// Copyright 2025 Bloxide, all rights reserved
//! Unit tests for the Counter blox.
//!
//! Run with: `cargo test -p counter-blox --features std`

#[cfg(all(test, feature = "std"))]
mod counter_tests {
    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::test_utils::TestRuntime;
    use bloxide_core::{spec::MachineSpec, Envelope, MachineState, StateMachine};
    use counter_actions::CountsTicks;
    use counter_messages::{CounterMsg, Tick};

    use crate::{CounterCtx, CounterEvent, CounterSpec, CounterState};

    // ── Test behavior type ───────────────────────────────────────────────────

    /// Simple behavior that stores a count.
    #[derive(Default)]
    struct TestBehavior {
        count: u8,
    }

    impl CountsTicks for TestBehavior {
        type Count = u8;

        fn count(&self) -> u8 {
            self.count
        }

        fn set_count(&mut self, count: u8) {
            self.count = count;
        }
    }

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn make_machine() -> StateMachine<CounterSpec<TestRuntime, TestBehavior>> {
        let ctx = CounterCtx::new(bloxide_core::next_actor_id!(), TestBehavior::default());
        StateMachine::new(ctx)
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_start_enters_ready() {
        let mut machine = make_machine();
        assert!(matches!(machine.current_state(), MachineState::Init));

        machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(CounterState::Ready)
        ));
    }

    #[test]
    fn test_tick_in_ready_stays() {
        let mut machine = make_machine();
        machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start));

        // First tick should stay in Ready (count becomes 1, threshold is 2)
        machine.dispatch(CounterEvent::Msg(Envelope(0, CounterMsg::Tick(Tick {}))));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(CounterState::Ready)
        ));
        assert_eq!(machine.ctx().behavior.count(), 1);
    }

    #[test]
    fn test_tick_reaches_done() {
        let mut machine = make_machine();
        machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start));

        // First tick
        machine.dispatch(CounterEvent::Msg(Envelope(0, CounterMsg::Tick(Tick {}))));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(CounterState::Ready)
        ));

        // Second tick should transition to Done (count >= 2)
        machine.dispatch(CounterEvent::Msg(Envelope(0, CounterMsg::Tick(Tick {}))));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(CounterState::Done)
        ));
    }

    #[test]
    fn test_done_is_terminal() {
        assert!(CounterSpec::<TestRuntime, TestBehavior>::is_terminal(
            &CounterState::Done
        ));
        assert!(!CounterSpec::<TestRuntime, TestBehavior>::is_terminal(
            &CounterState::Ready
        ));
    }
}
