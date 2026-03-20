// Copyright 2025 Bloxide, all rights reserved
// Tokio worker pool demo — dynamic actor creation with supervision and KillCap.
//
// Demonstrates:
//   1. Dynamic actor spawning: pool receives SpawnWorker → creates worker tasks at runtime
//   2. P2P peer introduction: pool wires workers to each other via PeerCtrl channels
//   3. Supervision: supervisor manages pool and worker lifecycles
//   4. KillCap: policy-driven cleanup for dynamic actors (workers use Kill policy)
//
// Concrete worker construction lives in tokio-pool-demo-impl (Layer 3). This
// binary remains Layer 5 wiring only. pool-blox and worker-blox are fully
// independent; they are coupled only through pool-actions and pool-messages.
//
// Run with: RUST_LOG=info cargo run --example tokio-pool-demo

use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_tokio::prelude::*;
use pool_blox::{PoolCtx, PoolSpec};
use pool_messages::{PoolMsg, SpawnWorker};
use std::sync::Arc;
use tokio_pool_demo_impl::spawn_worker_tokio;
use tracing_log::LogTracer;

bloxide_tokio::actor_task_supervised!(pool_task, PoolSpec<TokioRuntime>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

#[tokio::main]
async fn main() {
    LogTracer::init().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init()
        .ok();

    // Create KillCap for dynamic actor cleanup
    let kill_cap = Arc::new(bloxide_tokio::TokioKillCap::new());

    // Create the pool's mailbox
    let ((pool_ref,), pool_mbox) = bloxide_tokio::channels! { PoolMsg(32) };
    let pool_id = pool_ref.id();

    tracing::info!(pool_id, "pool created");

    // Build the pool machine
    let pool_ctx = PoolCtx::new(pool_id, pool_ref.clone(), spawn_worker_tokio);
    let pool_machine = StateMachine::<PoolSpec<TokioRuntime>>::new(pool_ctx);

    // Set up supervision with KillCap support
    // Pool uses Stop policy (clean shutdown), workers would use Kill policy (immediate cleanup)
    let mut group = ChildGroupBuilder::with_kill_cap(GroupShutdown::WhenAnyDone, kill_cap.clone());
    bloxide_tokio::spawn_child!(
        group,
        pool_task(pool_machine, pool_mbox, pool_id),
        ChildPolicy::Stop
    );
    let _sup_control_ref = group.control_ref();
    let _sup_notify = group.notify_sender();
    let sup_id = bloxide_tokio::next_actor_id!();
    let (children, sup_notify_rx, sup_control_rx) = group.finish();

    tracing::info!(sup_id, pool_id, "supervisor setup complete");

    // Build and start the supervisor
    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));

    // Pre-load SpawnWorker messages — pool will process them when it starts
    let worker_count = 3u32;
    for task_id in 0..worker_count {
        pool_ref
            .try_send(pool_id, PoolMsg::SpawnWorker(SpawnWorker { task_id }))
            .expect("pool mailbox not full");
    }
    tracing::info!(worker_count, "SpawnWorker messages queued");

    // Run the supervisor until shutdown
    supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;

    tracing::info!("pool demo complete");
}
