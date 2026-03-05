// Unit tests for the Ping blox.
//
// Each test corresponds to one acceptance criterion from `spec/bloxes/ping.md`.
// All tests use `TestRuntime` (virtual clock, in-memory queues) — no Embassy
// executor required.
//
// Run with: `cargo test -p ping-blox --features std`

#[cfg(all(test, feature = "std"))]
mod ping_tests {
    use crate::{PingCtx, PingSpec, PingState, MAX_ROUNDS, PAUSE_AT_ROUND, PAUSE_DURATION_MS};
    use bloxide_core::messaging::ActorId;
    use bloxide_core::test_utils::{TestReceiver, TestRuntime, TestSender};
    use bloxide_core::{spec::MachineSpec, DynamicChannelCap, Envelope, StateMachine};
    use bloxide_timer::{TimerCommand, TimerId};
    use ping_pong_actions::{CountsRounds, HasCurrentTimer};
    use ping_pong_messages::{Ping, PingPongMsg, Pong};
    use std::vec::Vec;

    // ── Local behavior implementation for tests ──────────────────────────────

    #[derive(Default, Clone)]
    struct TestBehavior {
        round: u32,
        current_timer: Option<TimerId>,
    }

    impl CountsRounds for TestBehavior {
        type Round = u32;
        fn round(&self) -> u32 {
            self.round
        }
        fn set_round(&mut self, round: u32) {
            self.round = round;
        }
    }

    impl HasCurrentTimer for TestBehavior {
        fn current_timer(&self) -> Option<TimerId> {
            self.current_timer
        }
        fn set_current_timer(&mut self, timer: Option<TimerId>) {
            self.current_timer = timer;
        }
    }

    // ── Inline timer simulation ──────────────────────────────────────────────

    struct PendingTimer {
        id: TimerId,
        deadline_ms: u64,
        deliver: std::boxed::Box<dyn FnOnce() + Send>,
    }

    // ── Test harness ─────────────────────────────────────────────────────────

    struct PingHarness {
        machine: StateMachine<PingSpec<TestRuntime, TestBehavior>>,
        ping_id: ActorId,
        to_ping_rx: TestReceiver<PingPongMsg>,
        to_pong_rx: TestReceiver<PingPongMsg>,
        timer_rx: TestReceiver<TimerCommand>,
        pending_timers: Vec<PendingTimer>,
        now_ms: u64,
    }

    impl PingHarness {
        fn new() -> Self {
            let ping_id = TestRuntime::alloc_actor_id();
            let (self_ref, to_ping_rx) =
                <TestRuntime as DynamicChannelCap>::channel::<PingPongMsg>(ping_id, 16);
            let pong_id = TestRuntime::alloc_actor_id();
            let (pong_ref, to_pong_rx) =
                <TestRuntime as DynamicChannelCap>::channel::<PingPongMsg>(pong_id, 16);
            let timer_id = TestRuntime::alloc_actor_id();
            let (timer_ref, timer_rx) =
                <TestRuntime as DynamicChannelCap>::channel::<TimerCommand>(timer_id, 16);

            let ctx = PingCtx::new(
                ping_id,
                pong_ref,
                self_ref,
                timer_ref,
                TestBehavior::default(),
            );
            let machine = StateMachine::<PingSpec<TestRuntime, TestBehavior>>::new(ctx);

            PingHarness {
                machine,
                ping_id,
                to_ping_rx,
                to_pong_rx,
                timer_rx,
                pending_timers: Vec::new(),
                now_ms: 0,
            }
        }

        fn start(&mut self) {
            self.machine.start();
        }

        fn send_pong(&mut self) {
            let round = self.ctx().round();
            self.machine
                .dispatch(Envelope(0, PingPongMsg::Pong(Pong { round })).into());
        }

        fn terminate(&mut self) {
            self.machine.reset();
        }

        /// Drain timer commands into the inline queue, advance simulated clock,
        /// and fire all callbacks whose deadline has been reached.
        fn advance_time(&mut self, ms: u64) {
            for cmd in self.timer_rx.drain_payloads() {
                match cmd {
                    TimerCommand::Set {
                        id,
                        after_ms,
                        deliver,
                    } => {
                        self.pending_timers.push(PendingTimer {
                            id,
                            deadline_ms: self.now_ms + after_ms,
                            deliver,
                        });
                    }
                    TimerCommand::Cancel { id } => {
                        self.pending_timers.retain(|t| t.id != id);
                    }
                    TimerCommand::Shutdown => {}
                }
            }

            self.now_ms += ms;
            let now = self.now_ms;

            let all = std::mem::take(&mut self.pending_timers);
            let mut fired: Vec<PendingTimer> = Vec::new();
            let mut remaining: Vec<PendingTimer> = Vec::new();
            for t in all {
                if t.deadline_ms <= now {
                    fired.push(t);
                } else {
                    remaining.push(t);
                }
            }
            self.pending_timers = remaining;
            fired.sort_by_key(|t| t.deadline_ms);
            for t in fired {
                (t.deliver)();
            }
        }

        fn dispatch_pending_self_msgs(&mut self) {
            let msgs = self.to_ping_rx.drain_payloads();
            let id = self.ping_id;
            for msg in msgs {
                self.machine.dispatch(Envelope(id, msg).into());
            }
        }

        fn drain_to_pong_rx(&mut self) -> Vec<PingPongMsg> {
            self.to_pong_rx.drain_payloads()
        }

        fn drain_to_ping_rx(&mut self) -> Vec<PingPongMsg> {
            self.to_ping_rx.drain_payloads()
        }

        fn current_state(&self) -> Option<PingState> {
            self.machine.current_state()
        }

        fn ctx(&self) -> &PingCtx<TestRuntime, TestBehavior> {
            self.machine.ctx()
        }
    }

    fn run_through_pause(h: &mut PingHarness) {
        h.start();
        h.drain_to_pong_rx();
        for _ in 1..PAUSE_AT_ROUND {
            h.send_pong();
            h.drain_to_pong_rx();
        }
        // Send pong at pause round → transitions to Paused
        h.send_pong();
        // Fire the resume timer → transitions back to Active
        h.advance_time(PAUSE_DURATION_MS);
        h.dispatch_pending_self_msgs();
        h.drain_to_pong_rx();
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[test]
    fn start_enters_active_and_sends_first_ping() {
        let mut h = PingHarness::new();
        h.start();

        assert_eq!(h.current_state(), Some(PingState::Active));
        assert_eq!(h.ctx().round(), 1);

        let sent = h.drain_to_pong_rx();
        assert_eq!(sent.len(), 1);
        assert!(matches!(sent[0], PingPongMsg::Ping(Ping { round: 1 })));
    }

    #[test]
    fn pong_response_advances_round() {
        let mut h = PingHarness::new();
        h.start();
        h.drain_to_pong_rx();

        h.send_pong();

        assert_eq!(h.ctx().round(), 2);

        let sent = h.drain_to_pong_rx();
        assert_eq!(
            sent.len(),
            1,
            "send_next_ping fires once in the transition action"
        );
        assert!(
            matches!(sent[0], PingPongMsg::Ping(Ping { round: 1 })),
            "ping carries the pre-increment round (sent by transition action before on_entry)"
        );
    }

    #[test]
    fn pong_response_at_pause_round_transitions_to_paused() {
        let mut h = PingHarness::new();
        h.start();
        h.drain_to_pong_rx();

        for _ in 1..PAUSE_AT_ROUND {
            h.send_pong();
            h.drain_to_pong_rx();
        }
        assert_eq!(h.ctx().round(), u32::from(PAUSE_AT_ROUND));

        h.send_pong();

        assert_eq!(h.current_state(), Some(PingState::Paused));
        assert!(
            h.ctx().behavior.current_timer.is_some(),
            "Paused::on_entry must set a timer"
        );

        h.advance_time(PAUSE_DURATION_MS);
        let resumes = h.drain_to_ping_rx();
        assert_eq!(
            resumes.len(),
            1,
            "exactly one Resume must arrive after the timer fires"
        );
        assert!(matches!(resumes[0], PingPongMsg::Resume(_)));
    }

    #[test]
    fn timer_fires_resume_transitions_to_active() {
        let mut h = PingHarness::new();
        h.start();
        h.drain_to_pong_rx();

        for _ in 1..PAUSE_AT_ROUND {
            h.send_pong();
            h.drain_to_pong_rx();
        }
        h.send_pong();

        assert_eq!(h.current_state(), Some(PingState::Paused));

        h.advance_time(PAUSE_DURATION_MS);
        h.dispatch_pending_self_msgs();

        let expected_round = u32::from(PAUSE_AT_ROUND) + 1;
        assert_eq!(h.current_state(), Some(PingState::Active));
        assert_eq!(h.ctx().round(), expected_round);

        let sent = h.drain_to_pong_rx();
        assert_eq!(
            sent.len(),
            2,
            "Ping from Pong→Paused transition + Ping from Resume→Active transition"
        );
        assert!(matches!(sent[0], PingPongMsg::Ping(Ping { round: r }) if r == u32::from(PAUSE_AT_ROUND)),
            "first ping carries the round at which the Pong transition fired (before on_entry increment)");
        assert!(matches!(sent[1], PingPongMsg::Ping(Ping { round: r }) if r == u32::from(PAUSE_AT_ROUND)),
            "resume ping carries the pre-increment round (sent by transition action before Active on_entry)");
    }

    #[test]
    fn done_after_max_rounds() {
        let mut h = PingHarness::new();
        run_through_pause(&mut h);

        while h.ctx().round() < u32::from(MAX_ROUNDS) {
            h.send_pong();
            h.drain_to_pong_rx();
        }
        h.send_pong();

        assert_eq!(h.current_state(), Some(PingState::Done));
        assert!(
            PingSpec::<TestRuntime, TestBehavior>::is_terminal(&PingState::Done),
            "is_terminal must return true for PingState::Done"
        );
    }

    #[test]
    fn error_state_is_error() {
        assert!(
            PingSpec::<TestRuntime, TestBehavior>::is_error(&PingState::Error),
            "is_error must return true for PingState::Error"
        );
        assert!(
            !PingSpec::<TestRuntime, TestBehavior>::is_error(&PingState::Active),
            "is_error must return false for non-error states"
        );
        assert!(
            !PingSpec::<TestRuntime, TestBehavior>::is_error(&PingState::Done),
            "is_error must return false for terminal states"
        );
    }

    #[test]
    fn terminate_resets_to_init() {
        let mut h = PingHarness::new();

        run_through_pause(&mut h);
        while h.ctx().round() < u32::from(MAX_ROUNDS) {
            h.send_pong();
            h.drain_to_pong_rx();
        }
        h.send_pong();

        assert_eq!(h.current_state(), Some(PingState::Done));

        h.terminate();

        assert_eq!(
            h.current_state(),
            None,
            "machine must be in Init after reset"
        );

        assert_eq!(h.ctx().round(), 0);

        h.send_pong();
        assert_eq!(
            h.current_state(),
            None,
            "non-Start events must be dropped in Init"
        );
    }

    /// AC: A stray `Pong` received while in `Paused` is absorbed by
    /// `Operating::transitions` (which has `PingPongMsg::Pong(_) => stay`), so
    /// the machine stays in `Paused` and no message is sent to the peer.
    #[test]
    fn stray_pong_in_paused_is_absorbed_by_operating() {
        let mut h = PingHarness::new();
        h.start();
        h.drain_to_pong_rx();

        // Drive to Paused
        for _ in 1..PAUSE_AT_ROUND {
            h.send_pong();
            h.drain_to_pong_rx();
        }
        h.send_pong();
        assert_eq!(h.current_state(), Some(PingState::Paused));
        h.drain_to_pong_rx();

        // Dispatch a stray Pong while in Paused
        h.machine
            .dispatch(Envelope(0, PingPongMsg::Pong(Pong { round: 99 })).into());

        assert_eq!(
            h.current_state(),
            Some(PingState::Paused),
            "stray Pong must not leave Paused"
        );
        let sent = h.drain_to_pong_rx();
        assert!(
            sent.is_empty(),
            "stray Pong in Paused must not send any message to the peer"
        );
    }

    #[test]
    fn pong_with_full_peer_channel_transitions_to_error() {
        let mut h = PingHarness::new();
        h.start();
        h.drain_to_pong_rx();

        let peer_sender: TestSender<PingPongMsg> = h.ctx().peer_ref.sender();
        peer_sender.set_full(true);

        h.machine
            .dispatch(Envelope(0, PingPongMsg::Pong(Pong { round: 1 })).into());

        assert_eq!(
            h.current_state(),
            Some(PingState::Error),
            "send_next_ping must fail when the peer channel is full, triggering Error via results.any_failed()"
        );
    }
}
