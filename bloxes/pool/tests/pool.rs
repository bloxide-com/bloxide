// Copyright 2025 Bloxide, all rights reserved
// Integration tests for the Pool blox (blox-level tier).
//
// These tests run against the blox-level `PoolSpec`, whose action closures
// are STUBS (no-ops returning `ActionResult::Ok`) while the guards are real.
// They therefore only cover what is true at this level:
//
// - topology: states, flat hierarchy, initial state
// - stub-level lifecycle: `Start` → `Idle`, guard-driven transitions that do
//   not depend on action side effects
//
// Behavior that lives in the concrete actions (spawn flow, spawn-queue
// processing, `DoWork` dispatch, `WorkDone` accounting) is NOT tested here —
// it is covered at the app level by `examples/tokio-pool-demo/tests/`, which
// drives the system-generated concrete specs. With stub actions the ctx
// fields (`pending`, `spawn_queue`, `spawn_in_flight`, `worker_refs`, …)
// never change from their defaults, and the tests below assert exactly that.
//
// The tests exercise the dynamic spawn machinery, so they require the
// crate's `dynamic` feature (part of the default feature set).
//
// Run with: `cargo blox generate`, then
// `cargo test --manifest-path target/bloxide-generated/Cargo.toml -p pool-blox`

use blox_ctx_pool_ref::{SpawnRequest, SpawnedWorker};
use bloxide_child_management::ChildCtrl;
use bloxide_child_management::ChildPolicy;
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    lifecycle::LifecycleCommand,
    messaging::ActorRef,
    spec::MachineSpec,
    topology::StateTopology,
    Envelope, MachineState, StateMachine,
};
use bloxide_peers::PeerCtrl;
use bloxide_spawn::{SpawnFn, SpawnOutput};
use bloxide_test_runtime::TestRuntime;
use pool_blox::{PoolCtx, PoolEvent, PoolSpec, PoolState};
use pool_messages::{PoolMsg, SpawnWorker, WorkDone, WorkerMsg};

// ── Test fixture ─────────────────────────────────────────────────────────

struct PoolHarness {
    machine: StateMachine<PoolSpec<TestRuntime>>,
    _control_rx: <TestRuntime as BloxRuntime>::Receiver<ChildCtrl<TestRuntime>>,
    _spawn_reply_ref:
        ActorRef<SpawnedWorker<PeerCtrl<WorkerMsg, TestRuntime>, TestRuntime>, TestRuntime>,
}

/// Dummy spawn function for tests.
///
/// Creates channels for the worker, sends a `SpawnedWorker` reply on the
/// request's `reply_to` channel, and returns a `SpawnOutput` with the
/// lifecycle/kill refs. The actual worker task is not spawned — tests
/// only verify the Pool's state-machine transitions.
fn test_spawn_worker(
    req: SpawnRequest<PeerCtrl<WorkerMsg, TestRuntime>, TestRuntime>,
    _notify: ActorRef<ChildLifecycleEvent, TestRuntime>,
) -> SpawnOutput<TestRuntime> {
    match req {
        SpawnRequest::Worker {
            task_id: _,
            reply_to,
            pool_ref: _,
        } => {
            let worker_id = TestRuntime::alloc_actor_id();
            let (domain_ref, _domain_rx) =
                <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 16);
            let (ctrl_ref, _ctrl_rx) = <TestRuntime as DynamicChannelCap>::channel::<
                PeerCtrl<WorkerMsg, TestRuntime>,
            >(worker_id, 16);
            let (lifecycle_ref, _lifecycle_rx) =
                <TestRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(worker_id, 4);
            let (abort_ref, _abort_rx) = <TestRuntime as DynamicChannelCap>::channel::<
                bloxide_core::lifecycle::AbortCommand,
            >(worker_id, 4);

            let _ = reply_to.try_send(
                worker_id,
                SpawnedWorker {
                    child_id: worker_id,
                    domain_ref: domain_ref.clone(),
                    ctrl_ref: ctrl_ref.clone(),
                },
            );

            SpawnOutput {
                child_id: worker_id,
                lifecycle_ref,
                abort_ref,
                kill_handle: 0,
                policy: ChildPolicy::Stop,
            }
        }
    }
}

impl PoolHarness {
    fn new() -> Self {
        let pool_id = TestRuntime::alloc_actor_id();
        let (pool_ref, _pool_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<PoolMsg>(pool_id, 32);

        let control_id = TestRuntime::alloc_actor_id();
        let (control_ref, control_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<ChildCtrl<TestRuntime>>(control_id, 16);

        let notify_id = TestRuntime::alloc_actor_id();
        let (notify_ref, _notify_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(notify_id, 16);

        let reply_id = TestRuntime::alloc_actor_id();
        let (spawn_reply_ref, _reply_rx) = <TestRuntime as DynamicChannelCap>::channel::<
            SpawnedWorker<PeerCtrl<WorkerMsg, TestRuntime>, TestRuntime>,
        >(reply_id, 16);

        let spawn_fn: SpawnFn<
            TestRuntime,
            SpawnRequest<PeerCtrl<WorkerMsg, TestRuntime>, TestRuntime>,
        > = test_spawn_worker;
        let ctx = PoolCtx::new(
            pool_id,
            pool_ref.clone(),
            spawn_fn,
            control_ref,
            notify_ref,
            spawn_reply_ref.clone(),
        );
        let machine = StateMachine::<PoolSpec<TestRuntime>>::new(ctx);

        PoolHarness {
            machine,
            _control_rx: control_rx,
            _spawn_reply_ref: spawn_reply_ref.clone(),
        }
    }

    fn start(&mut self) {
        self.machine
            .dispatch(PoolEvent::Lifecycle(LifecycleCommand::Start));
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
        domain_ref: ActorRef<WorkerMsg, TestRuntime>,
        ctrl_ref: ActorRef<PeerCtrl<WorkerMsg, TestRuntime>, TestRuntime>,
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
}

// Helper to create dummy worker refs for a reply.
fn dummy_worker_refs(
    worker_id: usize,
) -> (
    ActorRef<WorkerMsg, TestRuntime>,
    ActorRef<PeerCtrl<WorkerMsg, TestRuntime>, TestRuntime>,
) {
    let (domain_ref, _domain_rx) =
        <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 16);
    let (ctrl_ref, _ctrl_rx) = <TestRuntime as DynamicChannelCap>::channel::<
        PeerCtrl<WorkerMsg, TestRuntime>,
    >(worker_id, 16);
    (domain_ref, ctrl_ref)
}

// ── Topology tests ───────────────────────────────────────────────────────

#[test]
fn pool_topology_is_flat() {
    assert_eq!(PoolState::STATE_COUNT, 3);
    assert_eq!(PoolSpec::<TestRuntime>::initial_state(), PoolState::Idle);
    for state in [PoolState::Idle, PoolState::Spawning, PoolState::Active] {
        assert!(state.parent().is_none(), "{:?} must have no parent", state);
        assert!(state.is_leaf(), "{:?} must be a leaf state", state);
        assert!(
            !PoolSpec::<TestRuntime>::is_error(&state),
            "{:?} must not be an error state",
            state
        );
    }
    assert_eq!(PoolState::Idle.as_index(), 0);
    assert_eq!(PoolState::Spawning.as_index(), 1);
    assert_eq!(PoolState::Active.as_index(), 2);
}

// ── Stub-level lifecycle tests ───────────────────────────────────────────

#[test]
fn pool_starts_in_idle() {
    let mut h = PoolHarness::new();
    h.start();
    assert_eq!(h.current_state(), MachineState::State(PoolState::Idle));
}

#[test]
fn spawn_worker_transitions_idle_to_spawning() {
    let mut h = PoolHarness::new();
    h.start();

    h.dispatch_spawn_worker(1);

    assert_eq!(h.current_state(), MachineState::State(PoolState::Spawning));
    // Stub action is a no-op: no accounting happens at blox level.
    assert_eq!(h.machine.ctx().pending, 0);
    assert!(!h.machine.ctx().spawn_in_flight);
}

#[test]
fn spawn_worker_then_spawned_worker_transitions_to_active() {
    let mut h = PoolHarness::new();
    h.start();

    h.dispatch_spawn_worker(1);
    let (domain_ref, ctrl_ref) = dummy_worker_refs(1);
    h.dispatch_spawned_worker(1, domain_ref, ctrl_ref);

    // With stub actions spawn_in_flight stays false, spawn_queue stays
    // empty and worker_refs stays empty, so the guard takes the Active
    // branch. (The pending==0 && !worker_refs.is_empty() stop branch is
    // not reachable with stubs because worker_refs is never populated.)
    assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
    // Stub action is a no-op: pending is never incremented at blox level.
    assert_eq!(h.machine.ctx().pending, 0);
}

#[test]
fn multiple_spawn_workers_stay_active() {
    let mut h = PoolHarness::new();
    h.start();

    // Each SpawnWorker goes Active → Spawning; each SpawnReply returns to
    // Active (stub actions leave spawn_in_flight/spawn_queue untouched).
    for i in 1u32..=3 {
        let worker_id = i as usize;
        h.dispatch_spawn_worker(i);
        let (domain_ref, ctrl_ref) = dummy_worker_refs(worker_id);
        h.dispatch_spawned_worker(worker_id, domain_ref, ctrl_ref);
    }

    assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
    assert_eq!(h.machine.ctx().pending, 0);
}

#[test]
fn work_done_with_zero_pending_stops_to_init() {
    let mut h = PoolHarness::new();
    h.start();

    h.dispatch_spawn_worker(1);
    let (domain_ref, ctrl_ref) = dummy_worker_refs(1);
    h.dispatch_spawned_worker(1, domain_ref, ctrl_ref);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Active));

    // Drive the dynamic spawn fields to non-initial values (stub actions
    // never touch them, so set them directly): a stop must reset them.
    // None of these fields feed the Active + WorkDone guard below.
    h.machine.ctx_mut().pending_task_id = 7;
    h.machine.ctx_mut().spawn_in_flight = true;
    h.machine.ctx_mut().spawn_queue.push(42);

    // The Active + WorkDone guard is real: pending == 0 (stub actions
    // never increment it) → Decision::Stop → machine returns to Init.
    h.dispatch_work_done(1, 1, 0);
    assert!(
        h.current_state().is_init(),
        "machine must be in Init after WorkDone with pending == 0 (Decision::Stop)"
    );

    // Entering Init fires on_init_entry: the dynamic spawn state must be
    // reset so a restart does not leak stale spawn bookkeeping.
    assert_eq!(h.machine.ctx().pending_task_id, 0);
    assert!(!h.machine.ctx().spawn_in_flight);
    assert!(h.machine.ctx().spawn_queue.is_empty());

    // The machine can be restarted: Start from Init re-enters Idle.
    h.start();
    assert_eq!(h.current_state(), MachineState::State(PoolState::Idle));
}

#[test]
fn work_done_in_spawning_is_absorbed() {
    let mut h = PoolHarness::new();
    h.start();

    h.dispatch_spawn_worker(1);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Spawning));

    // Spawning + WorkDone → stay (real guard, no action side effects).
    h.dispatch_work_done(1, 1, 0);
    assert_eq!(h.current_state(), MachineState::State(PoolState::Spawning));
}
