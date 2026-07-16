// Copyright 2025 Bloxide, all rights reserved
//! Unit tests for the Pool blox.
//!
//! Uses `TestRuntime` to verify the pool state-machine behavior for the
//! supervisor-based 2-phase spawn model. Spawn requests are sent to a mock
//! spawn mailbox and replies are injected via `SpawnReply` events.
//!
//! Run with: `cargo test -p pool-blox --features std`

#[cfg(all(test, feature = "std"))]
mod pool_tests {
    use bloxide_core::test_utils::TestRuntime;
    use bloxide_core::{
        capability::{BloxRuntime, DynamicChannelCap},
        lifecycle::LifecycleCommand,
        messaging::ActorRef,
        spec::MachineSpec,
        Envelope, MachineState, StateMachine,
    };
    use pool_messages::{
        AppSpawnRequest, PoolMsg, SpawnWorker, SpawnedWorker, WorkDone, WorkerCtrl, WorkerMsg,
    };

    use crate::{PoolCtx, PoolEvent, PoolSpec, PoolState};

    // ── Test fixture ─────────────────────────────────────────────────────────

    struct PoolHarness {
        machine: StateMachine<PoolSpec<TestRuntime>>,
        _spawn_rx: <TestRuntime as BloxRuntime>::Receiver<AppSpawnRequest<TestRuntime>>,
        _spawn_reply_ref: ActorRef<SpawnedWorker<TestRuntime>, TestRuntime>,
    }

    impl PoolHarness {
        fn new() -> Self {
            let pool_id = TestRuntime::alloc_actor_id();
            let (pool_ref, _pool_rx) =
                <TestRuntime as DynamicChannelCap>::channel::<PoolMsg>(pool_id, 32);

            let spawn_id = TestRuntime::alloc_actor_id();
            let (spawn_ref, spawn_rx) = <TestRuntime as DynamicChannelCap>::channel::<
                AppSpawnRequest<TestRuntime>,
            >(spawn_id, 16);

            let reply_id = TestRuntime::alloc_actor_id();
            let (spawn_reply_ref, _reply_rx) = <TestRuntime as DynamicChannelCap>::channel::<
                SpawnedWorker<TestRuntime>,
            >(reply_id, 16);

            let ctx = PoolCtx::new(
                pool_ref.clone(),
                pool_id,
                spawn_ref,
                spawn_reply_ref.clone(),
            );
            let machine = StateMachine::<PoolSpec<TestRuntime>>::new(ctx);

            PoolHarness {
                machine,
                _spawn_rx: spawn_rx,
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
            ctrl_ref: ActorRef<WorkerCtrl<TestRuntime>, TestRuntime>,
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
    }

    // Helper to create dummy worker refs for a reply.
    fn dummy_worker_refs(
        worker_id: usize,
    ) -> (
        ActorRef<WorkerMsg, TestRuntime>,
        ActorRef<WorkerCtrl<TestRuntime>, TestRuntime>,
    ) {
        let (domain_ref, _domain_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 16);
        let (ctrl_ref, _ctrl_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerCtrl<TestRuntime>>(worker_id, 16);
        (domain_ref, ctrl_ref)
    }

    // ── Tests ────────────────────────────────────────────────────────────────

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
        assert_eq!(h.pending(), 0);
    }

    #[test]
    fn spawn_worker_then_spawned_worker_transitions_to_active() {
        let mut h = PoolHarness::new();
        h.start();

        h.dispatch_spawn_worker(1);
        let (domain_ref, ctrl_ref) = dummy_worker_refs(1);
        h.dispatch_spawned_worker(1, domain_ref, ctrl_ref);

        assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
        assert_eq!(h.pending(), 1);
    }

    #[test]
    fn multiple_spawn_workers_stay_active() {
        let mut h = PoolHarness::new();
        h.start();

        for i in 1u32..=3 {
            let worker_id = i as usize;
            h.dispatch_spawn_worker(i);
            let (domain_ref, ctrl_ref) = dummy_worker_refs(worker_id);
            h.dispatch_spawned_worker(worker_id, domain_ref, ctrl_ref);
        }

        assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
        assert_eq!(h.pending(), 3);
    }

    #[test]
    fn work_done_decrements_pending() {
        let mut h = PoolHarness::new();
        h.start();

        h.dispatch_spawn_worker(10);
        let (domain_ref, ctrl_ref) = dummy_worker_refs(1);
        h.dispatch_spawned_worker(1, domain_ref, ctrl_ref);

        h.dispatch_spawn_worker(20);
        let (domain_ref2, ctrl_ref2) = dummy_worker_refs(2);
        h.dispatch_spawned_worker(2, domain_ref2, ctrl_ref2);

        assert_eq!(h.pending(), 2);

        h.dispatch_work_done(1, 10, 20);
        assert_eq!(h.pending(), 1);
        assert_eq!(h.current_state(), MachineState::State(PoolState::Active));
    }

    #[test]
    fn all_work_done_transitions_to_all_done() {
        let mut h = PoolHarness::new();
        h.start();

        h.dispatch_spawn_worker(1);
        let (domain_ref, ctrl_ref) = dummy_worker_refs(1);
        h.dispatch_spawned_worker(1, domain_ref, ctrl_ref);

        h.dispatch_spawn_worker(2);
        let (domain_ref2, ctrl_ref2) = dummy_worker_refs(2);
        h.dispatch_spawned_worker(2, domain_ref2, ctrl_ref2);

        h.dispatch_work_done(1, 1, 2);
        assert_eq!(h.current_state(), MachineState::State(PoolState::Active));

        h.dispatch_work_done(2, 2, 4);
        assert_eq!(h.current_state(), MachineState::State(PoolState::AllDone));
        assert!(PoolSpec::<TestRuntime>::is_terminal(&PoolState::AllDone));
    }

    #[test]
    fn pool_stores_worker_refs() {
        let mut h = PoolHarness::new();
        h.start();

        h.dispatch_spawn_worker(1);
        let (domain_ref, ctrl_ref) = dummy_worker_refs(1);
        h.dispatch_spawned_worker(1, domain_ref, ctrl_ref);

        h.dispatch_spawn_worker(2);
        let (domain_ref2, ctrl_ref2) = dummy_worker_refs(2);
        h.dispatch_spawned_worker(2, domain_ref2, ctrl_ref2);

        assert_eq!(
            h.machine.ctx().worker_refs.len(),
            2,
            "pool should store refs for all spawned workers"
        );
    }

    #[test]
    fn spawned_worker_with_full_domain_channel_decrements_pending() {
        let pool_id = TestRuntime::alloc_actor_id();
        let (pool_ref, _pool_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<PoolMsg>(pool_id, 32);

        let spawn_id = TestRuntime::alloc_actor_id();
        let (spawn_ref, _spawn_rx) = <TestRuntime as DynamicChannelCap>::channel::<
            AppSpawnRequest<TestRuntime>,
        >(spawn_id, 16);

        let reply_id = TestRuntime::alloc_actor_id();
        let (spawn_reply_ref, _reply_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<SpawnedWorker<TestRuntime>>(reply_id, 16);

        let ctx = PoolCtx::new(pool_ref.clone(), pool_id, spawn_ref, spawn_reply_ref);
        let mut machine = StateMachine::<PoolSpec<TestRuntime>>::new(ctx);
        machine.dispatch(PoolEvent::Lifecycle(LifecycleCommand::Start));

        machine.dispatch(PoolEvent::Msg(Envelope(
            0,
            PoolMsg::SpawnWorker(SpawnWorker { task_id: 42 }),
        )));

        let worker_id = 1usize;
        let (domain_ref, _domain_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 16);
        let (ctrl_ref, _ctrl_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerCtrl<TestRuntime>>(worker_id, 16);
        domain_ref.sender().set_full(true);

        machine.dispatch(PoolEvent::SpawnReply(Envelope(
            0,
            SpawnedWorker {
                child_id: worker_id,
                domain_ref,
                ctrl_ref,
            },
        )));

        assert_eq!(
            machine.ctx().pending,
            0,
            "pending should be decremented back to 0 when DoWork send fails"
        );
        assert_eq!(
            machine.ctx().worker_refs.len(),
            1,
            "worker ref should still be stored even though DoWork was dropped"
        );
    }
}
