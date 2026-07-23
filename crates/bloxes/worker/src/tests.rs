// Copyright 2025 Bloxide, all rights reserved
//! Unit tests for the Worker blox.
//!
//! Run with: `cargo test -p worker-blox --features std`

#[cfg(all(test, feature = "std"))]
mod worker_tests {
    extern crate alloc;
    use alloc::vec::Vec;

    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::{capability::DynamicChannelCap, Envelope, MachineState, StateMachine};
    use bloxide_peers::{AddPeer, PeerCtrl};
    use bloxide_test_runtime::{TestReceiver, TestRuntime};
    use pool_messages::{DoWork, PeerResult, PoolMsg, WorkDone, WorkerMsg};

    use crate::prelude::*;

    struct WorkerHarness {
        machine: StateMachine<WorkerSpec<TestRuntime>>,
        pool_rx: TestReceiver<PoolMsg>,
    }

    impl WorkerHarness {
        fn new() -> Self {
            let worker_id = TestRuntime::alloc_actor_id();
            let pool_id = TestRuntime::alloc_actor_id();

            let (pool_ref, pool_rx) =
                <TestRuntime as DynamicChannelCap>::channel::<PoolMsg>(pool_id, 16);

            let ctx = WorkerCtx::new(worker_id, pool_ref, Vec::new(), 0, 0);
            let machine = StateMachine::<WorkerSpec<TestRuntime>>::new(ctx);

            WorkerHarness { machine, pool_rx }
        }

        fn start(&mut self) {
            self.machine
                .dispatch(WorkerEvent::Lifecycle(LifecycleCommand::Start));
        }

        fn dispatch_do_work(&mut self, task_id: u32) {
            self.machine
                .dispatch(Envelope(0, WorkerMsg::DoWork(DoWork { task_id })).into());
        }

        fn current_state(&self) -> MachineState<WorkerState> {
            self.machine.current_state()
        }

        fn drain_pool_msgs(&mut self) -> std::vec::Vec<PoolMsg> {
            self.pool_rx.drain_payloads()
        }

        fn peer_count(&self) -> usize {
            self.machine.ctx().peers.len()
        }
    }

    // ── State transition tests (work with stub actions) ────────────────────

    #[test]
    fn worker_starts_in_waiting() {
        let mut h = WorkerHarness::new();
        h.start();
        assert_eq!(h.current_state(), MachineState::State(WorkerState::Waiting));
    }

    #[test]
    fn do_work_transitions_to_stop() {
        let mut h = WorkerHarness::new();
        h.start();
        h.dispatch_do_work(7);
        // Guard::Stop fires after stub actions — machine returns to Init.
        assert!(
            h.current_state().is_init(),
            "machine must be in Init after DoWork (Guard::Stop)"
        );
    }

    #[test]
    fn peer_result_in_waiting_is_ignored() {
        let mut h = WorkerHarness::new();
        h.start();

        h.machine.dispatch(
            Envelope(
                0,
                WorkerMsg::PeerResult(PeerResult {
                    from_id: 99,
                    result: 42,
                }),
            )
            .into(),
        );
        assert_eq!(h.current_state(), MachineState::State(WorkerState::Waiting));
    }

    // ── Action function tests (real impls in actions.rs, stubs in spec) ────

    #[test]
    fn handle_ctrl_adds_peer_to_ctx() {
        let mut h = WorkerHarness::new();
        h.start();

        let peer_id = TestRuntime::alloc_actor_id();
        let (peer_ref, _peer_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(peer_id, 16);

        assert_eq!(h.peer_count(), 0);

        // Call handle_ctrl action directly (stub in spec, real impl in actions.rs)
        let ev: WorkerEvent<TestRuntime> =
            Envelope(0, PeerCtrl::AddPeer(AddPeer { peer_id, peer_ref })).into();
        WorkerSpec::<TestRuntime>::handle_ctrl(h.machine.ctx_mut(), &ev);
        assert_eq!(h.peer_count(), 1);
    }

    #[test]
    fn process_work_sets_task_id_and_result() {
        let mut h = WorkerHarness::new();
        h.start();

        let ev: WorkerEvent<TestRuntime> =
            Envelope(0, WorkerMsg::DoWork(DoWork { task_id: 5 })).into();
        WorkerSpec::<TestRuntime>::process_work(h.machine.ctx_mut(), &ev);

        assert_eq!(h.machine.ctx().task_id, 5);
        assert_eq!(h.machine.ctx().result, 10, "result = task_id * 2");
    }

    #[test]
    fn do_notify_pool_sends_work_done() {
        let mut h = WorkerHarness::new();
        h.start();

        // Set task state manually (stub actions don't set it)
        h.machine.ctx_mut().task_id = 5;
        h.machine.ctx_mut().result = 10;

        let ev: WorkerEvent<TestRuntime> =
            Envelope(0, WorkerMsg::DoWork(DoWork { task_id: 5 })).into();
        WorkerSpec::<TestRuntime>::do_notify_pool(h.machine.ctx_mut(), &ev);

        let msgs = h.drain_pool_msgs();
        assert_eq!(msgs.len(), 1, "exactly one WorkDone should be sent");
        match &msgs[0] {
            PoolMsg::WorkDone(WorkDone {
                task_id, result, ..
            }) => {
                assert_eq!(*task_id, 5);
                assert_eq!(*result, 10, "result = task_id * 2");
            }
            other => panic!("expected WorkDone, got {:?}", other as *const _),
        }
    }

    #[test]
    fn do_broadcast_sends_peer_result_to_all_peers() {
        let mut h = WorkerHarness::new();
        h.start();

        let peer1_id = TestRuntime::alloc_actor_id();
        let peer2_id = TestRuntime::alloc_actor_id();
        let (peer1_ref, mut peer1_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(peer1_id, 16);
        let (peer2_ref, mut peer2_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(peer2_id, 16);

        // Add peers manually (stub actions don't handle ctrl)
        h.machine.ctx_mut().peers.push(peer1_ref);
        h.machine.ctx_mut().peers.push(peer2_ref);
        assert_eq!(h.peer_count(), 2);

        // Set result manually (stub actions don't process work)
        h.machine.ctx_mut().result = 6;

        let ev: WorkerEvent<TestRuntime> =
            Envelope(0, WorkerMsg::DoWork(DoWork { task_id: 3 })).into();
        WorkerSpec::<TestRuntime>::do_broadcast(h.machine.ctx_mut(), &ev);

        let p1_msgs = peer1_rx.drain_payloads();
        let p2_msgs = peer2_rx.drain_payloads();
        assert_eq!(p1_msgs.len(), 1, "peer1 should receive one PeerResult");
        assert_eq!(p2_msgs.len(), 1, "peer2 should receive one PeerResult");

        assert!(
            matches!(
                p1_msgs[0],
                WorkerMsg::PeerResult(PeerResult { result: 6, .. })
            ),
            "peer1 result should be 6"
        );
        assert!(
            matches!(
                p2_msgs[0],
                WorkerMsg::PeerResult(PeerResult { result: 6, .. })
            ),
            "peer2 result should be 6"
        );
    }
}
