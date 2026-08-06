// Copyright 2025 Bloxide, all rights reserved
//! Behavior tests for the Pool, driven against the SYSTEM-LEVEL concrete
//! `PoolSpec` generated from `system.toml` (`src/generated/`).
//!
//! Unlike the blox-crate-level spec (stub actions), the concrete spec wires
//! the real action functions from `tokio-pool-demo-impl`: spawn accounting
//! (`pending`, `spawn_in_flight`, `spawn_queue`), worker-ref storage, `DoWork`
//! dispatch, and `WorkDone` accounting are all exercised here.
//!
//! Each test drives the state machine synchronously via `dispatch()`:
//!
//! - `SpawnWorker` / `WorkDone` messages are dispatched as `PoolMsg` events.
//! - The pool's real spawn factory (`tokio_pool_demo_impl::build_worker`
//!   composed with `bloxide_spawn::spawn_actor_task`) runs inside the
//!   actions, spawning actual (never-started) worker tasks
//!   and sending `RegisterDynamicChild` to a dummy supervisor control
//!   channel — no supervisor consumes it.
//! - `SpawnedWorker` replies are INJECTED manually with test-made worker refs
//!   (deterministic — the factory's real replies sit unread in the reply
//!   channel), so each test controls exactly when the pool learns about a
//!   spawned worker.
//!
//! The tests are `#[tokio::test]` because the platform spawn helper calls
//! `tokio::spawn`; the test bodies themselves are fully synchronous.
//!
//! Run with: `cargo test -p tokio-pool-demo`

use blox_ctx_pool_ref::SpawnedWorker;
use bloxide_child_management::control::ChildCtrl;
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    messaging::ActorRef,
    Envelope, MachineState, StateMachine,
};
use bloxide_peers::PeerCtrl;
use bloxide_tokio::TokioRuntime;
use pool_blox::{PoolCtx, PoolEvent, PoolState};
use pool_messages::{PeerResult, PoolMsg, SpawnWorker, WorkDone, WorkerMsg};

use tokio_pool_demo::generated::pool_spec_skeleton::PoolSpec;
use tokio_pool_demo::generated::worker_spec_skeleton::WorkerSpec;

type SpawnReplyMsg = SpawnedWorker<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>;

// ── Test fixture ─────────────────────────────────────────────────────────────

struct PoolHarness {
    machine: StateMachine<PoolSpec<TokioRuntime>>,
    // Receivers are held so the channels never close; their contents are not
    // read (the spawn factory's real SpawnedWorker replies stay here unread).
    _pool_rx: <TokioRuntime as BloxRuntime>::Receiver<PoolMsg>,
    _control_rx: <TokioRuntime as BloxRuntime>::Receiver<ChildCtrl<TokioRuntime>>,
    _notify_rx: <TokioRuntime as BloxRuntime>::Receiver<ChildLifecycleEvent>,
    _reply_rx: <TokioRuntime as BloxRuntime>::Receiver<SpawnReplyMsg>,
}

impl PoolHarness {
    fn new() -> Self {
        let pool_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (pool_ref, pool_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<PoolMsg>(pool_id, 32);

        let control_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (control_ref, control_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<ChildCtrl<TokioRuntime>>(control_id, 16);

        let notify_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (notify_ref, notify_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(notify_id, 16);

        let reply_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (spawn_reply_ref, reply_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<SpawnReplyMsg>(reply_id, 16);

        let ctx = PoolCtx::new(
            pool_id,
            pool_ref,
            // The real spawn factory, monomorphized with the concrete
            // system-level WorkerSpec — same wiring as src/main.rs: domain
            // build (impl crate) composed with the platform spawn helper.
            (|req, notify| {
                ::bloxide_spawn::spawn_actor_task(
                    ::tokio_pool_demo_impl::build_worker::<WorkerSpec<TokioRuntime>>(req),
                    notify,
                )
            }) as _,
            control_ref,
            notify_ref,
            spawn_reply_ref,
        );
        let machine = StateMachine::<PoolSpec<TokioRuntime>>::new(ctx);

        PoolHarness {
            machine,
            _pool_rx: pool_rx,
            _control_rx: control_rx,
            _notify_rx: notify_rx,
            _reply_rx: reply_rx,
        }
    }

    fn start(&mut self) {
        self.machine.dispatch(PoolEvent::Lifecycle(
            bloxide_core::lifecycle::LifecycleCommand::Start,
        ));
    }

    fn dispatch_spawn_worker(&mut self, task_id: u32) {
        self.machine.dispatch(PoolEvent::Msg(Envelope(
            0,
            PoolMsg::SpawnWorker(SpawnWorker { task_id }),
        )));
    }

    fn dispatch_spawned_worker(
        &mut self,
        worker_id: usize,
        domain_ref: ActorRef<WorkerMsg, TokioRuntime>,
        ctrl_ref: ActorRef<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>,
    ) {
        self.machine.dispatch(PoolEvent::SpawnReply(Envelope(
            0,
            SpawnedWorker {
                child_id: worker_id,
                domain_ref,
                ctrl_ref,
            },
        )));
    }

    fn dispatch_work_done(&mut self, worker_id: usize, task_id: u32, result: u32) {
        self.machine.dispatch(PoolEvent::Msg(Envelope(
            worker_id,
            PoolMsg::WorkDone(WorkDone {
                worker_id,
                task_id,
                result,
            }),
        )));
    }

    fn current_state(&self) -> MachineState<PoolState> {
        self.machine.current_state()
    }

    fn pending(&self) -> u32 {
        self.machine.ctx().pending
    }

    /// Spawn a worker for `task_id` (dispatch `SpawnWorker`) and immediately
    /// inject the `SpawnedWorker` reply for it.
    fn spawn_and_reply(&mut self, task_id: u32, worker_id: usize) {
        self.dispatch_spawn_worker(task_id);
        let (domain_ref, ctrl_ref) = dummy_worker_refs(worker_id);
        self.dispatch_spawned_worker(worker_id, domain_ref, ctrl_ref);
    }
}

/// Create idle worker refs for an injected reply (capacity 16 — sends never
/// fill the channel).
fn dummy_worker_refs(
    worker_id: usize,
) -> (
    ActorRef<WorkerMsg, TokioRuntime>,
    ActorRef<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>,
) {
    let (domain_ref, _domain_rx) =
        <TokioRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 16);
    let (ctrl_ref, _ctrl_rx) = <TokioRuntime as DynamicChannelCap>::channel::<
        PeerCtrl<WorkerMsg, TokioRuntime>,
    >(worker_id, 16);
    (domain_ref, ctrl_ref)
}

/// Create worker refs whose domain channel is already full (capacity 1,
/// pre-filled), so the `DoWork` `try_send` to this worker fails — models a
/// saturated worker.
fn saturated_worker_refs(
    worker_id: usize,
) -> (
    ActorRef<WorkerMsg, TokioRuntime>,
    ActorRef<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>,
) {
    let (domain_ref, _domain_rx) =
        <TokioRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 1);
    domain_ref
        .try_send(
            0,
            WorkerMsg::PeerResult(PeerResult {
                from_id: 0,
                result: 0,
            }),
        )
        .expect("pre-fill must succeed");
    let (ctrl_ref, _ctrl_rx) = <TokioRuntime as DynamicChannelCap>::channel::<
        PeerCtrl<WorkerMsg, TokioRuntime>,
    >(worker_id, 16);
    (domain_ref, ctrl_ref)
}

// ── WorkDone accounting ──────────────────────────────────────────────────────

#[tokio::test]
async fn work_done_decrements_pending() {
    let mut h = PoolHarness::new();
    h.start();

    h.spawn_and_reply(10, 1);
    h.spawn_and_reply(20, 2);

    assert_eq!(h.pending(), 2);

    h.dispatch_work_done(1, 10, 20);
    assert_eq!(h.pending(), 1);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
}

#[tokio::test]
async fn all_work_done_transitions_to_stop() {
    let mut h = PoolHarness::new();
    h.start();

    h.spawn_and_reply(1, 1);
    h.spawn_and_reply(2, 2);

    h.dispatch_work_done(1, 1, 2);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Active));

    h.dispatch_work_done(2, 2, 4);
    // All workers done (pending == 0) → Decision::Stop → machine returns to
    // Init; on_init_entry clears the worker refs.
    assert!(
        h.current_state().is_init(),
        "machine must be in Init after all workers done (Decision::Stop)"
    );
    assert_eq!(h.machine.ctx().worker_refs.len(), 0);
}

#[tokio::test]
async fn pool_stores_worker_refs() {
    let mut h = PoolHarness::new();
    h.start();

    h.spawn_and_reply(1, 1);
    h.spawn_and_reply(2, 2);

    assert_eq!(
        h.machine.ctx().worker_refs.len(),
        2,
        "pool should store refs for all spawned workers"
    );
    assert_eq!(h.machine.ctx().worker_ctrls.len(), 2);
}

#[tokio::test]
async fn spawned_worker_with_full_domain_channel_keeps_pending() {
    let mut h = PoolHarness::new();
    h.start();

    h.dispatch_spawn_worker(42);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Spawning));

    // The DoWork try_send to this worker fails (channel full).
    let (domain_ref, ctrl_ref) = saturated_worker_refs(1);
    h.dispatch_spawned_worker(1, domain_ref, ctrl_ref);

    // Current semantics: the failed send surfaces as ActionResult::Err, but
    // the SpawnReply guard does not inspect action results, so the pool
    // carries on to Active. The work item is still owed — pending stays at 1
    // and the dropped DoWork means this worker will never report WorkDone.
    assert_eq!(
        h.pending(),
        1,
        "pending is not adjusted when the DoWork send fails"
    );
    assert_eq!(h.machine.ctx().worker_refs.len(), 1);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
}

// ── Spawn queue processing ───────────────────────────────────────────────────

#[tokio::test]
async fn spawn_worker_while_spawning_is_queued() {
    let mut h = PoolHarness::new();
    h.start();

    // First SpawnWorker → Spawning
    h.dispatch_spawn_worker(0);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Spawning));

    // Additional SpawnWorkers while in Spawning are buffered (stay Spawning)
    h.dispatch_spawn_worker(1);
    h.dispatch_spawn_worker(2);
    assert_eq!(
        h.current_state(),
        MachineState::State(PoolState::Spawning),
        "pool should stay in Spawning when buffering additional spawn requests"
    );
    assert_eq!(
        h.machine.ctx().spawn_queue.len(),
        2,
        "two spawn requests should be queued"
    );
    // Every request (in-flight or queued) counts as pending work.
    assert_eq!(h.pending(), 3);
}

#[tokio::test]
async fn queued_spawns_are_processed_after_spawn_reply() {
    let mut h = PoolHarness::new();
    h.start();

    // Send 3 SpawnWorker requests: first transitions to Spawning, other 2 are queued
    h.dispatch_spawn_worker(0);
    h.dispatch_spawn_worker(1);
    h.dispatch_spawn_worker(2);
    assert_eq!(h.machine.ctx().spawn_queue.len(), 2);

    // First SpawnReply (for task_id=0): pops queued task_id=1, stays in Spawning
    let (dr1, cr1) = dummy_worker_refs(1);
    h.dispatch_spawned_worker(1, dr1, cr1);
    assert_eq!(
        h.current_state(),
        MachineState::State(PoolState::Spawning),
        "should stay in Spawning because task_id=1 spawn is now in-flight"
    );
    assert_eq!(h.machine.ctx().spawn_queue.len(), 1);
    assert_eq!(h.pending(), 3, "no WorkDone yet — all 3 tasks still owed");

    // Second SpawnReply (for task_id=1): pops queued task_id=2, stays in Spawning
    let (dr2, cr2) = dummy_worker_refs(2);
    h.dispatch_spawned_worker(2, dr2, cr2);
    assert_eq!(
        h.current_state(),
        MachineState::State(PoolState::Spawning),
        "should stay in Spawning because task_id=2 spawn is in-flight"
    );
    assert_eq!(h.machine.ctx().spawn_queue.len(), 0);
    assert_eq!(h.pending(), 3);

    // Third SpawnReply (for task_id=2): queue empty, no in-flight spawn → Active
    let (dr3, cr3) = dummy_worker_refs(3);
    h.dispatch_spawned_worker(3, dr3, cr3);
    assert_eq!(
        h.current_state(),
        MachineState::State(PoolState::Active),
        "should transition to Active after all queued spawns are processed"
    );
    assert_eq!(h.machine.ctx().spawn_queue.len(), 0);
    assert_eq!(h.pending(), 3);
}

#[tokio::test]
async fn work_done_in_spawning_state_stays_in_spawning() {
    let mut h = PoolHarness::new();
    h.start();

    // Spawn 2 workers: first goes to Spawning, second is queued
    h.dispatch_spawn_worker(0);
    h.dispatch_spawn_worker(1);

    // First worker is spawned; the queued task_id=1 spawn starts (in-flight)
    let (dr1, cr1) = dummy_worker_refs(1);
    h.dispatch_spawned_worker(1, dr1, cr1);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Spawning));
    assert_eq!(h.pending(), 2);

    // First worker finishes while pool is still Spawning (second spawn in-flight)
    h.dispatch_work_done(1, 0, 0);
    assert_eq!(
        h.current_state(),
        MachineState::State(PoolState::Spawning),
        "WorkDone in Spawning should stay in Spawning"
    );
    assert_eq!(h.pending(), 1);
}

#[tokio::test]
async fn full_three_worker_flow_with_queue() {
    let mut h = PoolHarness::new();
    h.start();

    // Send all 3 SpawnWorker messages at once
    h.dispatch_spawn_worker(0);
    h.dispatch_spawn_worker(1);
    h.dispatch_spawn_worker(2);
    assert_eq!(h.machine.ctx().spawn_queue.len(), 2);

    // Worker 1 spawned (for task_id=0), queued task_id=1 starts
    let (dr1, cr1) = dummy_worker_refs(1);
    h.dispatch_spawned_worker(1, dr1, cr1);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Spawning));
    assert_eq!(h.pending(), 3);

    // Worker 1 finishes while Spawning
    h.dispatch_work_done(1, 0, 0);
    assert_eq!(h.pending(), 2);

    // Worker 2 spawned (for task_id=1), queued task_id=2 starts
    let (dr2, cr2) = dummy_worker_refs(2);
    h.dispatch_spawned_worker(2, dr2, cr2);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Spawning));
    assert_eq!(h.pending(), 2);

    // Worker 2 finishes while Spawning
    h.dispatch_work_done(2, 1, 2);
    assert_eq!(h.pending(), 1);

    // Worker 3 spawned (for task_id=2), queue empty → Active
    let (dr3, cr3) = dummy_worker_refs(3);
    h.dispatch_spawned_worker(3, dr3, cr3);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
    assert_eq!(h.pending(), 1);

    // Worker 3 finishes → Decision::Stop → Init
    h.dispatch_work_done(3, 2, 4);
    assert!(
        h.current_state().is_init(),
        "machine must be in Init after all workers done (Decision::Stop)"
    );
    // on_init_entry clears worker_refs on Decision::Stop.
    assert_eq!(
        h.machine.ctx().worker_refs.len(),
        0,
        "worker refs cleared by on_init_entry after Decision::Stop"
    );
}
