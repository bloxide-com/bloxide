// Copyright 2025 Bloxide, all rights reserved
//! Unit tests for the Worker blox.
//!
//! Run with: `cargo test -p worker-blox --features std`

#[cfg(all(test, feature = "std"))]
mod worker_tests {
    extern crate alloc;

    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::{capability::DynamicChannelCap, Envelope, MachineState, StateMachine};
    use bloxide_peers::{AddPeer, PeerCtrl};
    use bloxide_test_runtime::{TestReceiver, TestRuntime};
    use pool_messages::{DoWork, PeerResult, PoolMsg, WorkerMsg};

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

            let ctx = WorkerCtx::new(worker_id, pool_ref);
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
    fn do_work_transitions_to_done() {
        let mut h = WorkerHarness::new();
        h.start();
        h.dispatch_do_work(7);
        // Decision::Done fires after stub actions — same cleanup as Stop
        // (machine parks in Init), then the run loop ends the task and the
        // supervisor deregisters the worker (task-end is a run-loop concern).
        assert!(
            h.current_state().is_init(),
            "machine must be in Init after DoWork (Decision::Done)"
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

    // ── Action function tests (stub actions in the blox-crate spec) ───────
    // Blox-crate-level codegen always emits stub no-op action closures —
    // the concrete wiring comes from the system-level codegen (system.toml).
    // These tests verify that the state fields are accessible and manually
    // settable, which is what the stub-based tests rely on.

    #[test]
    fn handle_ctrl_stub_does_not_add_peer() {
        let mut h = WorkerHarness::new();
        h.start();

        let peer_id = TestRuntime::alloc_actor_id();
        let (peer_ref, _peer_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(peer_id, 16);

        assert_eq!(h.peer_count(), 0);

        // Dispatch AddPeer — stub action is a no-op, so peers stays empty
        h.machine
            .dispatch(Envelope(0, PeerCtrl::AddPeer(AddPeer { peer_id, peer_ref })).into());
        assert_eq!(h.peer_count(), 0, "stub action does not add peer");
    }

    #[test]
    fn process_work_stub_does_not_set_task_id() {
        let mut h = WorkerHarness::new();
        h.start();

        // Dispatch DoWork — stub action is a no-op, so task_id stays 0
        h.dispatch_do_work(5);
        assert_eq!(
            h.machine.ctx().task_id,
            0,
            "stub action does not set task_id"
        );
        assert_eq!(h.machine.ctx().result, 0, "stub action does not set result");
    }

    #[test]
    fn do_notify_pool_stub_does_not_send_work_done() {
        let mut h = WorkerHarness::new();
        h.start();

        // Set task state manually (stub actions don't set it)
        h.machine.ctx_mut().task_id = 5;
        h.machine.ctx_mut().result = 10;

        // Dispatch DoWork — stub action is a no-op, no WorkDone sent
        h.dispatch_do_work(5);
        let msgs = h.drain_pool_msgs();
        assert_eq!(msgs.len(), 0, "stub action does not send WorkDone");
    }

    #[test]
    fn do_broadcast_stub_does_not_send_peer_result() {
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

        // Dispatch DoWork — stub action is a no-op, no PeerResult sent
        h.dispatch_do_work(3);

        let p1_msgs = peer1_rx.drain_payloads();
        let p2_msgs = peer2_rx.drain_payloads();
        assert_eq!(p1_msgs.len(), 0, "stub action does not send PeerResult");
        assert_eq!(p2_msgs.len(), 0, "stub action does not send PeerResult");
    }

    // ── Mailbox priority ───────────────────────────────────────────────────
    //
    // The Worker's `Mailboxes` tuple declares the Ctrl stream at index 0
    // (highest priority — see generated/spec_skeleton.rs), so all pending
    // `AddPeer` messages are processed before `DoWork` is dispatched.

    #[test]
    fn ctrl_mailbox_is_polled_before_domain_mailbox() {
        use bloxide_core::mailboxes::Mailboxes;
        use bloxide_core::spec::MachineSpec;
        use std::task::{Context, Poll};

        fn noop_waker() -> std::task::Waker {
            use std::task::{RawWaker, RawWakerVTable, Waker};
            static VTABLE: RawWakerVTable = RawWakerVTable::new(
                |_| RawWaker::new(std::ptr::null(), &VTABLE),
                |_| {},
                |_| {},
                |_| {},
            );
            unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
        }

        let worker_id = TestRuntime::alloc_actor_id();
        let (ctrl_ref, ctrl_rx) = <TestRuntime as DynamicChannelCap>::channel::<
            PeerCtrl<WorkerMsg, TestRuntime>,
        >(worker_id, 16);
        let (msg_ref, msg_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 16);

        // Queue the domain message FIRST, then the ctrl message.
        msg_ref
            .try_send(0, WorkerMsg::DoWork(DoWork { task_id: 1 }))
            .unwrap();
        let peer_id = TestRuntime::alloc_actor_id();
        let (peer_ref, _peer_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<WorkerMsg>(peer_id, 16);
        ctrl_ref
            .try_send(0, PeerCtrl::AddPeer(AddPeer { peer_id, peer_ref }))
            .unwrap();

        // The spec's declared mailbox set — index 0 (Ctrl) is polled first.
        let mut mailboxes: <WorkerSpec<TestRuntime> as MachineSpec>::Mailboxes<TestRuntime> =
            (ctrl_rx, msg_rx);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let first: Poll<Option<WorkerEvent<TestRuntime>>> = mailboxes.poll_next(&mut cx);
        assert!(
            matches!(first, Poll::Ready(Some(WorkerEvent::Ctrl(_)))),
            "ctrl message must be polled before the earlier-queued domain message"
        );

        let second: Poll<Option<WorkerEvent<TestRuntime>>> = mailboxes.poll_next(&mut cx);
        assert!(
            matches!(second, Poll::Ready(Some(WorkerEvent::Msg(_)))),
            "domain message must be polled after the ctrl message"
        );
    }
}
