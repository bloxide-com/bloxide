// Copyright 2025 Bloxide, all rights reserved
//! Unit tests for the Counter blox.
//!
//! Run with: `cargo test -p counter-blox --features std`

#[cfg(all(test, feature = "std"))]
mod counter_tests {
    use blox_ctx_ticks::increment_count;
    use bloxide_core::lifecycle::LifecycleCommand;
    use bloxide_core::{Envelope, MachineState, StateMachine};
    use counter_messages::{CounterMsg, Tick};

    use crate::{CounterCtx, CounterEvent, CounterSpec, CounterState};

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn make_machine() -> StateMachine<CounterSpec> {
        let ctx = CounterCtx::new(bloxide_core::next_actor_id!(), 0);
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

        // First tick: stub action is a no-op, so we increment manually
        // to verify guard logic. The guard checks ctx.count >= 2.
        increment_count(&mut machine.ctx_mut().count);
        machine.dispatch(CounterEvent::Msg(Envelope(0, CounterMsg::Tick(Tick {}))));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(CounterState::Ready)
        ));
        assert_eq!(machine.ctx().count, 1);
    }

    #[test]
    fn test_tick_reaches_stop() {
        let mut machine = make_machine();
        machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start));

        // Manually set count to 1, then dispatch tick — guard sees count >= 2
        // after stub action (which is a no-op in blox-level spec).
        // Actually, the guard fires BEFORE actions in the current engine?
        // Let's test with count = 2 directly.
        machine.ctx_mut().count = 2;
        machine.dispatch(CounterEvent::Msg(Envelope(0, CounterMsg::Tick(Tick {}))));
        assert!(matches!(machine.current_state(), MachineState::Init));
    }

    #[test]
    fn test_increment_count_function() {
        let mut count = 0u8;
        increment_count(&mut count);
        assert_eq!(count, 1);
        increment_count(&mut count);
        assert_eq!(count, 2);
    }
}
