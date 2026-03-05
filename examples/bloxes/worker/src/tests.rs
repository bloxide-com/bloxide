//! Unit tests for the Worker blox.
//!
//! Run with: `cargo test -p worker-blox --features std`

#[cfg(all(test, feature = "std"))]
mod worker_tests {
    use bloxide_core::test_utils::{TestReceiver, TestRuntime};
    use bloxide_core::{capability::DynamicChannelCap, spec::MachineSpec, Envelope, StateMachine};
    use bloxide_spawn::peer::{AddPeer, PeerCtrl};
    use pool_messages::{DoWork, PeerResult, PoolMsg, WorkDone, WorkerMsg};

    use crate::{WorkerCtx, WorkerSpec, WorkerState};

    // ── Test fixture ─────────────────────────────────────────────────────────

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

            let ctx = WorkerCtx::new(worker_id, pool_ref);
            let machine = StateMachine::<WorkerSpec<TestRuntime>>::new(ctx);

            WorkerHarness { machine, pool_rx }
        }

        fn start(&mut self) {
            self.machine.start();
        }

        fn dispatch_do_work(&mut self, task_id: u32) {
            self.machine
                .dispatch(Envelope(0, WorkerMsg::DoWork(DoWork { task_id })).into());
        }

        fn dispatch_add_peer(
            &mut self,
            peer_ref: bloxide_core::messaging::ActorRef<WorkerMsg, TestRuntime>,
        ) {
            self.machine
                .dispatch(Envelope(0, PeerCtrl::AddPeer(AddPeer { peer_ref })).into());
        }

        fn current_state(&self) -> Option<WorkerState> {
            self.machine.current_state()
        }

        fn drain_pool_msgs(&mut self) -> std::vec::Vec<PoolMsg> {
            self.pool_rx.drain_payloads()
        }

        fn peer_count(&self) -> usize {
            self.machine.ctx().peers.len()
        }
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[test]
    fn worker_starts_in_waiting() {
        let mut h = WorkerHarness::new();
        h.start();
        assert_eq!(h.current_state(), Some(WorkerState::Waiting));
    }

    #[test]
    fn do_work_transitions_to_done() {
        let mut h = WorkerHarness::new();
        h.start();
        h.dispatch_do_work(7);
        assert_eq!(h.current_state(), Some(WorkerState::Done));
        assert!(WorkerSpec::<TestRuntime>::is_terminal(&WorkerState::Done));
    }

    #[test]
    fn done_notifies_pool_with_correct_result() {
        let mut h = WorkerHarness::new();
        h.start();
        h.dispatch_do_work(5);

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
    fn add_peer_is_stored_in_ctx() {
        let mut h = WorkerHarness::new();
        h.start();

        // Create a fake peer ActorRef
        let peer_id = TestRuntime::alloc_actor_id();
        let (peer_ref, _peer_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(peer_id, 16);

        assert_eq!(h.peer_count(), 0);
        h.dispatch_add_peer(peer_ref);
        assert_eq!(h.peer_count(), 1);
    }

    #[test]
    fn broadcast_sends_peer_result_to_all_peers() {
        let mut h = WorkerHarness::new();
        h.start();

        // Register two peers
        let peer1_id = TestRuntime::alloc_actor_id();
        let peer2_id = TestRuntime::alloc_actor_id();
        let (peer1_ref, mut peer1_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(peer1_id, 16);
        let (peer2_ref, mut peer2_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(peer2_id, 16);

        h.dispatch_add_peer(peer1_ref);
        h.dispatch_add_peer(peer2_ref);
        assert_eq!(h.peer_count(), 2);

        h.dispatch_do_work(3);
        assert_eq!(h.current_state(), Some(WorkerState::Done));

        // Both peers should have received a PeerResult
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

    #[test]
    fn peer_result_in_waiting_is_ignored() {
        let mut h = WorkerHarness::new();
        h.start();

        // PeerResult while in Waiting should be silently dropped (stay)
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
        assert_eq!(h.current_state(), Some(WorkerState::Waiting));
    }
}
