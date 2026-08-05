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
        let ctx = CounterCtx::new(bloxide_core::next_actor_id!());
        StateMachine::new(ctx)
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn start_enters_ready() {
        let mut machine = make_machine();
        assert!(matches!(machine.current_state(), MachineState::Init));

        machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(CounterState::Ready)
        ));
    }

    #[test]
    fn tick_in_ready_stays() {
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
    fn tick_reaches_done() {
        let mut machine = make_machine();
        machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start));

        // With count >= 2, the guard returns Decision::Done — the machine fires
        // the exit chain + on_init_entry and reports DispatchOutcome::Done
        // (clean self-termination; the run loop ends the task).
        machine.ctx_mut().count = 2;
        let outcome = machine.dispatch(CounterEvent::Msg(Envelope(0, CounterMsg::Tick(Tick {}))));
        assert!(matches!(
            outcome,
            bloxide_core::engine::DispatchOutcome::Done
        ));
        assert!(matches!(machine.current_state(), MachineState::Init));
    }

    #[test]
    fn reset_returns_to_ready_without_resetting_count() {
        let mut machine = make_machine();
        machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start));

        // Stub action is a no-op, so increment manually (as in the tick tests).
        increment_count(&mut machine.ctx_mut().count);
        assert_eq!(machine.ctx().count, 1);

        let outcome = machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Reset));
        assert!(matches!(
            outcome,
            bloxide_core::engine::DispatchOutcome::Started(MachineState::State(
                CounterState::Ready
            ))
        ));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(CounterState::Ready)
        ));

        // count is NOT reset: Reset skips Init entirely, so
        // on_init_entry (count = 0) never fires, and Ready has no on_entry.
        assert_eq!(machine.ctx().count, 1);
    }

    #[test]
    fn unhandled_event_in_init_is_dropped() {
        // CounterMsg has a single variant (Tick) and Ready handles it, so the
        // only state with no matching rule is Init. Domain events dispatched
        // while in Init are caught by Init's engine-generated catch-all and
        // silently dropped — no transition, no count change.
        let mut machine = make_machine();
        assert!(matches!(machine.current_state(), MachineState::Init));

        let outcome = machine.dispatch(CounterEvent::Msg(Envelope(0, CounterMsg::Tick(Tick {}))));
        assert!(matches!(
            outcome,
            bloxide_core::engine::DispatchOutcome::HandledNoTransition
        ));
        assert!(matches!(machine.current_state(), MachineState::Init));
        assert_eq!(machine.ctx().count, 0);

        // The machine still starts and processes subsequent valid events.
        machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start));
        machine.dispatch(CounterEvent::Msg(Envelope(0, CounterMsg::Tick(Tick {}))));
        assert!(matches!(
            machine.current_state(),
            MachineState::State(CounterState::Ready)
        ));
    }

    #[test]
    fn increment_count_function() {
        let mut count = 0u32;
        increment_count(&mut count);
        assert_eq!(count, 1);
        increment_count(&mut count);
        assert_eq!(count, 2);
    }
}
