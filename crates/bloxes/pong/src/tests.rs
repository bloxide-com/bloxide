// Copyright 2025 Bloxide, all rights reserved
// Unit tests for the Pong blox.
//
// Each test corresponds to one acceptance criterion from `spec/bloxes/pong.md`.
// All tests use `TestRuntime` (in-memory queues) — no Embassy executor required.
//
// Run with: `cargo test -p pong-blox --features std`

#[cfg(all(test, feature = "std"))]
mod pong_tests {
    use crate::{PongCtx, PongEvent, PongSpec, PongState};
    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::messaging::Envelope;
    use bloxide_core::spec::MachineSpec;
    use bloxide_core::test_utils::{TestReceiver, TestRuntime, TestSender};
    use bloxide_core::{DynamicChannelCap, MachineState, StateMachine};
    use ping_pong_messages::{Ping, PingPongMsg, Pong};
    use std::vec::Vec;

    struct PongHarness {
        machine: StateMachine<PongSpec<TestRuntime>>,
        to_ping_rx: TestReceiver<PingPongMsg>,
        ping_sender: TestSender<PingPongMsg>,
    }

    impl PongHarness {
        fn new() -> Self {
            let pong_id = TestRuntime::alloc_actor_id();
            let ping_id = TestRuntime::alloc_actor_id();
            let (ping_ref, to_ping_rx) =
                <TestRuntime as DynamicChannelCap>::channel::<PingPongMsg>(ping_id, 16);
            let ping_sender = ping_ref.sender();

            let ctx = PongCtx::new(pong_id, ping_ref);
            let machine = StateMachine::<PongSpec<TestRuntime>>::new(ctx);

            PongHarness {
                machine,
                to_ping_rx,
                ping_sender,
            }
        }

        fn start(&mut self) {
            self.machine
                .dispatch(PongEvent::Lifecycle(LifecycleCommand::Start));
        }

        fn terminate(&mut self) {
            self.machine
                .dispatch(PongEvent::Lifecycle(LifecycleCommand::Reset));
        }

        fn send_ping(&mut self, n: u32) {
            self.machine.dispatch(PongEvent::Msg(Envelope(
                0,
                PingPongMsg::Ping(Ping { round: n }),
            )));
        }

        fn drain_to_ping_rx(&mut self) -> Vec<PingPongMsg> {
            self.to_ping_rx.drain_payloads()
        }

        fn current_state(&self) -> MachineState<PongState> {
            self.machine.current_state()
        }
    }

    #[test]
    fn start_enters_ready() {
        let mut h = PongHarness::new();
        h.start();

        assert_eq!(h.current_state(), MachineState::State(PongState::Ready));
    }

    #[test]
    fn ping_in_ready_sends_pong_response() {
        let mut h = PongHarness::new();
        h.start();

        h.send_ping(3);

        assert_eq!(
            h.current_state(),
            MachineState::State(PongState::Ready),
            "Pong must stay in Ready"
        );

        let replies = h.drain_to_ping_rx();
        assert_eq!(replies.len(), 1);
        assert!(
            matches!(replies[0], PingPongMsg::Pong(Pong { round: 3 })),
            "must echo the same round number"
        );
    }

    #[test]
    fn pong_echoes_round_number_correctly() {
        let mut h = PongHarness::new();
        h.start();

        for n in [1u32, 2, 4, 7] {
            h.send_ping(n);
            let replies = h.drain_to_ping_rx();
            assert_eq!(replies.len(), 1);
            assert!(matches!(replies[0], PingPongMsg::Pong(Pong { round: r }) if r == n));
        }

        assert_eq!(h.current_state(), MachineState::State(PongState::Ready));
    }

    /// AC: When `send_pong` fails (peer channel full), the guard's
    /// `results.any_failed()` branch triggers and the machine transitions to
    /// `PongState::Error`.
    #[test]
    fn pong_with_full_peer_channel_transitions_to_error() {
        let mut h = PongHarness::new();
        h.start();

        h.ping_sender.set_full(true);

        h.send_ping(1);

        assert_eq!(
            h.current_state(),
            MachineState::State(PongState::Error),
            "send_pong failure must transition Pong to Error"
        );
        assert!(
            PongSpec::<TestRuntime>::is_error(&PongState::Error),
            "is_error must be true for PongState::Error"
        );
    }

    #[test]
    fn terminate_resets_to_init() {
        let mut h = PongHarness::new();
        h.start();

        assert_eq!(h.current_state(), MachineState::State(PongState::Ready));

        h.terminate();

        assert_eq!(
            h.current_state(),
            MachineState::Init,
            "machine must be in Init after reset"
        );

        h.send_ping(42);
        assert_eq!(
            h.current_state(),
            MachineState::Init,
            "non-Start events must be dropped in Init"
        );
        let replies = h.drain_to_ping_rx();
        assert!(
            replies.is_empty(),
            "no reply should be sent for events dropped in Init"
        );
    }
}
