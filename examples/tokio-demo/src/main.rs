// Tokio runtime demo — fully event-driven.
//
// Mirrors the embassy-demo but wired with TokioRuntime instead of EmbassyRuntime.
// No Spawner, no #[embassy_executor::task], no static_cell — just tokio::spawn
// and #[tokio::main].
//
// Run with: RUST_LOG=trace cargo run --bin tokio-demo

use bloxide_tokio::prelude::*;
use embassy_demo_impl::PingBehavior;
use ping_blox::prelude::*;
use ping_pong_messages::prelude::*;
use pong_blox::prelude::*;

async fn ping_task(
    machine: StateMachine<PingSpec<TokioRuntime, PingBehavior>>,
    domain_mailboxes: <PingSpec<TokioRuntime, PingBehavior> as MachineSpec>::Mailboxes<
        TokioRuntime,
    >,
    lifecycle_rx: bloxide_tokio::TokioStream<LifecycleCommand>,
    actor_id: ActorId,
    supervisor_notify: bloxide_tokio::TokioSender<ChildLifecycleEvent>,
) {
    <TokioRuntime as SupervisedRunLoop>::run_supervised_actor(
        machine,
        domain_mailboxes,
        lifecycle_rx,
        actor_id,
        supervisor_notify,
    )
    .await;
}

async fn pong_task(
    machine: StateMachine<PongSpec<TokioRuntime>>,
    domain_mailboxes: <PongSpec<TokioRuntime> as MachineSpec>::Mailboxes<TokioRuntime>,
    lifecycle_rx: bloxide_tokio::TokioStream<LifecycleCommand>,
    actor_id: ActorId,
    supervisor_notify: bloxide_tokio::TokioSender<ChildLifecycleEvent>,
) {
    <TokioRuntime as SupervisedRunLoop>::run_supervised_actor(
        machine,
        domain_mailboxes,
        lifecycle_rx,
        actor_id,
        supervisor_notify,
    )
    .await;
}

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
    let pong_ctx = PongCtx::new(pong_id, ping_ref);

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
    let sup_id = bloxide_tokio::next_actor_id!();
    let (children, sup_notify_rx) = group.finish();

    tracing::info!(sup_id, "supervisor setup");

    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    sup_machine.start();

    run_root(sup_machine, (sup_notify_rx,)).await;
}
