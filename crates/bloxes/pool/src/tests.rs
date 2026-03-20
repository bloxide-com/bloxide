// Copyright 2025 Bloxide, all rights reserved
//! Unit tests for the Pool blox.
//!
//! Uses `TestRuntime` + `SpawnCap for TestRuntime` from `bloxide-spawn`.
//! Spawned futures are captured in a thread-local vec and never actually run,
//! so tests verify pool state-machine behavior without running worker tasks.
//!
//! Run with: `cargo test -p pool-blox --features std`

#[cfg(all(test, feature = "std"))]
mod pool_tests {
    use bloxide_core::test_utils::TestRuntime;
    use bloxide_core::{
        capability::DynamicChannelCap, lifecycle::LifecycleCommand, messaging::ActorRef,
        spec::MachineSpec, Envelope, MachineState, StateMachine,
    };
    use bloxide_spawn::{test_impl::drain_spawned, SpawnCap};
    use pool_messages::{PoolMsg, SpawnWorker, WorkDone, WorkerCtrl, WorkerMsg};

    use crate::{PoolCtx, PoolEvent, PoolSpec, PoolState};

    // ── Mock worker factory ──────────────────────────────────────────────────
    //
    // Creates channels for a dummy worker and queues a no-op future, but never
    // constructs a real WorkerCtx or WorkerSpec. This keeps pool tests isolated
    // from worker-blox entirely.

    fn mock_spawn_worker(
        _pool_id: bloxide_core::messaging::ActorId,
        _pool_ref: &ActorRef<PoolMsg, TestRuntime>,
    ) -> (
        ActorRef<WorkerMsg, TestRuntime>,
        ActorRef<WorkerCtrl<TestRuntime>, TestRuntime>,
    ) {
        let wid = TestRuntime::alloc_actor_id();
        let (domain_ref, _domain_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(wid, 16);
        let (ctrl_ref, _ctrl_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerCtrl<TestRuntime>>(wid, 16);
        TestRuntime::spawn(async {});
        (domain_ref, ctrl_ref)
    }

    // ── Test fixture ─────────────────────────────────────────────────────────

    struct PoolHarness {
        machine: StateMachine<PoolSpec<TestRuntime>>,
    }

    impl PoolHarness {
        fn new() -> Self {
            let pool_id = TestRuntime::alloc_actor_id();
            let (pool_ref, _pool_rx) =
                <TestRuntime as DynamicChannelCap>::channel::<PoolMsg>(pool_id, 32);

            let ctx = PoolCtx::new(pool_id, pool_ref.clone(), mock_spawn_worker);
            let machine = StateMachine::<PoolSpec<TestRuntime>>::new(ctx);

            PoolHarness { machine }
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
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[test]
    fn pool_starts_in_idle() {
        let mut h = PoolHarness::new();
        h.start();
        assert_eq!(h.current_state(), MachineState::State(PoolState::Idle));
    }

    #[test]
    fn spawn_worker_transitions_idle_to_active() {
        drain_spawned(); // clear any leftovers from earlier tests
        let mut h = PoolHarness::new();
        h.start();

        h.dispatch_spawn_worker(1);

        assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
        assert_eq!(h.pending(), 1);
        assert_eq!(
            bloxide_spawn::test_impl::spawned_count(),
            1,
            "one worker future should be queued"
        );
        drain_spawned();
    }

    #[test]
    fn multiple_spawn_workers_stay_active() {
        drain_spawned();
        let mut h = PoolHarness::new();
        h.start();

        h.dispatch_spawn_worker(1);
        h.dispatch_spawn_worker(2);
        h.dispatch_spawn_worker(3);

        assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
        assert_eq!(h.pending(), 3);
        assert_eq!(
            bloxide_spawn::test_impl::spawned_count(),
            3,
            "three worker futures should be queued"
        );
        drain_spawned();
    }

    #[test]
    fn work_done_decrements_pending() {
        drain_spawned();
        let mut h = PoolHarness::new();
        h.start();

        h.dispatch_spawn_worker(10);
        h.dispatch_spawn_worker(20);
        assert_eq!(h.pending(), 2);
        drain_spawned();

        h.dispatch_work_done(1, 10, 20);
        assert_eq!(h.pending(), 1);
        assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
    }

    #[test]
    fn all_work_done_transitions_to_all_done() {
        drain_spawned();
        let mut h = PoolHarness::new();
        h.start();

        h.dispatch_spawn_worker(1);
        h.dispatch_spawn_worker(2);
        drain_spawned();

        h.dispatch_work_done(1, 1, 2);
        assert_eq!(h.current_state(), MachineState::State(PoolState::Active));

        h.dispatch_work_done(2, 2, 4);
        assert_eq!(h.current_state(), MachineState::State(PoolState::AllDone));
        assert!(PoolSpec::<TestRuntime>::is_terminal(&PoolState::AllDone));
    }

    #[test]
    fn pool_stores_worker_refs() {
        drain_spawned();
        let mut h = PoolHarness::new();
        h.start();

        h.dispatch_spawn_worker(1);
        h.dispatch_spawn_worker(2);
        drain_spawned();

        assert_eq!(
            h.machine.ctx().worker_refs.len(),
            2,
            "pool should store refs for all spawned workers"
        );
    }
}
