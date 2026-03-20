// Copyright 2025 Bloxide, all rights reserved
// Tokio runtime demo — fully event-driven.
//
// Mirrors the embassy-demo but wired with TokioRuntime instead of EmbassyRuntime.
// No Spawner, no #[embassy_executor::task], no static_cell — just tokio::spawn
// and #[tokio::main].
//
// This demo also shows:
// - supervisor control-plane health ticks (SupervisorControl::HealthCheckTick)
// - dynamic supervised child registration (SupervisorControl::RegisterChild)
//
// Run with: RUST_LOG=trace cargo run --example tokio-demo

use bloxide_tokio::prelude::*;
use embassy_demo_impl::PingBehavior;
use ping_blox::prelude::*;
use ping_pong_messages::prelude::*;
use pong_blox::prelude::*;
use std::time::Duration;

use bloxide_core::lifecycle::LifecycleCommand;

bloxide_tokio::actor_task_supervised!(ping_task, PingSpec<TokioRuntime, PingBehavior>);
bloxide_tokio::actor_task_supervised!(pong_task, PongSpec<TokioRuntime>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

#[tokio::main]
async fn main() {
    // LogTracer bridges `log`-crate records into the tracing pipeline.
    // `.ok()` tolerates a second init if another crate already registered a log backend.
    tracing_log::LogTracer::init().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("trace")),
        )
        .try_init()
        .ok();

    let timer_ref = bloxide_tokio::spawn_timer!(8);

    let ((ping_ref,), ping_mbox) = bloxide_tokio::channels! {
        PingPongMsg(16),
    };
    let ping_id = ping_ref.id();

    let ((pong_ref,), pong_mbox) = bloxide_tokio::channels! {
        PingPongMsg(16),
    };
    let pong_id = pong_ref.id();

    tracing::info!(ping_id, pong_id, "setup");

    let ping_ctx = PingCtx::new(
        ping_id,
        pong_ref.clone(),
        ping_ref.clone(),
        timer_ref,
        PingBehavior::default(),
    );
    let pong_ctx = PongCtx::new(pong_id, ping_ref.clone());

    let ping_machine = StateMachine::new(ping_ctx);
    let pong_machine = StateMachine::new(pong_ctx);

    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
    bloxide_tokio::spawn_child!(
        group,
        ping_task(ping_machine, ping_mbox, ping_id),
        ChildPolicy::Restart { max: 1 }
    );
    bloxide_tokio::spawn_child!(
        group,
        pong_task(pong_machine, pong_mbox, pong_id),
        ChildPolicy::Stop
    );
    let sup_control_ref = group.control_ref();
    let sup_notify = group.notify_sender();
    let sup_id = bloxide_tokio::next_actor_id!();
    let (children, sup_notify_rx, sup_control_rx) = group.finish();

    tracing::info!(sup_id, "supervisor setup");

    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    )); // root drives its own start; children get Start via LifecycleCommand

    // Keep health checks external to the core supervisor: this task ticks the
    // supervisor control-plane channel periodically.
    let health_ref = sup_control_ref.clone();
    let _health_task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        loop {
            ticker.tick().await;
            if health_ref
                .try_send(sup_id, SupervisorControl::HealthCheckTick)
                .is_err()
            {
                break;
            }
        }
    });

    // Demonstrate dynamic supervised registration by adding one extra child
    // after the static group is already running.
    let ((pong2_ref,), pong2_mbox) = bloxide_tokio::channels! {
        PingPongMsg(16),
    };
    let pong2_id = pong2_ref.id();
    let pong2_ctx = PongCtx::new(pong2_id, ping_ref);
    let pong2_machine = StateMachine::new(pong2_ctx);
    bloxide_tokio::spawn_child_dynamic!(
        sup_id,
        sup_control_ref,
        sup_notify,
        pong_task(pong2_machine, pong2_mbox, pong2_id),
        ChildPolicy::Stop
    )
    .expect("supervisor control channel should accept dynamic registration");

    supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;
}
