// Tokio worker pool demo — dynamic actor creation via SpawnCap.
//
// Demonstrates:
//   1. Dynamic actor spawning: pool receives SpawnWorker → creates worker tasks at runtime
//   2. P2P peer introduction: pool wires workers to each other via PeerCtrl channels
//   3. Self-sender invariant: pool holds worker ActorRefs keeping channels open
//
// The concrete worker type (WorkerCtx / WorkerSpec) is only referenced here in
// the wiring layer. pool-blox and worker-blox are fully independent; they are
// coupled only through pool-actions (traits) and pool-messages (data).
//
// Run with: RUST_LOG=info cargo run --bin tokio-pool-demo

use bloxide_core::{
    messaging::{ActorId, ActorRef},
    run_actor_to_completion, StateMachine,
};
use bloxide_spawn::{peer::PeerCtrl, SpawnCap};
use bloxide_tokio::{channels, TokioRuntime};
use pool_blox::{PoolCtx, PoolSpec};
use pool_messages::{PoolMsg, SpawnWorker, WorkerMsg};
use tracing_log::LogTracer;
use worker_blox::{WorkerCtx, WorkerSpec};

// ── Worker factory ───────────────────────────────────────────────────────────
//
// This function is the only place that knows about the concrete WorkerCtx /
// WorkerSpec types. It is injected into PoolCtx at startup so pool-blox never
// has to reference worker-blox directly.

fn spawn_worker(
    _pool_id: ActorId,
    pool_ref: &ActorRef<PoolMsg, TokioRuntime>,
) -> (
    ActorRef<WorkerMsg, TokioRuntime>,
    ActorRef<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>,
) {
    // Ctrl channel at index 0 (higher priority) so AddPeer messages are
    // processed before DoWork arrives on the domain channel.
    let ((ctrl_ref, domain_ref), worker_mbox) =
        channels! { PeerCtrl<WorkerMsg, TokioRuntime>(16), WorkerMsg(16) };
    // Each channels! call allocates a fresh ActorId per channel (ctrl and domain
    // get different IDs). We use ctrl_ref.id() as the canonical worker identity
    // because the worker's context is constructed with this ID, and logs/traces
    // emitted by the worker reference this ID via ctx.self_id().
    let worker_id = ctrl_ref.id();

    let worker_ctx = WorkerCtx::new(worker_id, pool_ref.clone());
    let machine = StateMachine::<WorkerSpec<TokioRuntime>>::new(worker_ctx);

    TokioRuntime::spawn(async move {
        run_actor_to_completion(machine, worker_mbox).await;
    });

    (domain_ref, ctrl_ref)
}

// ── Main ─────────────────────────────────────────────────────────────────────

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

    // Create the pool's mailbox.
    let ((pool_ref,), pool_mbox) = channels! { PoolMsg(32) };
    let pool_id = pool_ref.id();

    tracing::info!(pool_id, "pool created");

    // Pre-load SpawnWorker messages — pool will process them when it starts.
    let worker_count = 3u32;
    for task_id in 0..worker_count {
        pool_ref
            .try_send(pool_id, PoolMsg::SpawnWorker(SpawnWorker { task_id }))
            .expect("pool mailbox not full");
    }
    tracing::info!(worker_count, "SpawnWorker messages queued");

    // Build and run the pool to completion.
    // `spawn_worker` is injected so the pool never references worker-blox directly.
    let pool_ctx = PoolCtx::new(pool_id, pool_ref, spawn_worker);
    let pool_machine = StateMachine::<PoolSpec<TokioRuntime>>::new(pool_ctx);

    run_actor_to_completion(pool_machine, pool_mbox).await;

    tracing::info!("pool reached AllDone — exiting");
}
