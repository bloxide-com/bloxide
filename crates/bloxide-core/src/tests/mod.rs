// Copyright 2025 Bloxide, all rights reserved
//! HSM engine unit tests for unified lifecycle model.

#[cfg(feature = "std")]
pub(crate) mod fixture;

#[cfg(all(test, feature = "std"))]
mod hsm_engine {
    use super::fixture::*;
    use crate::engine::{DispatchOutcome, MachineState, StateMachine};
    use crate::lifecycle::LifecycleCommand;
    use crate::spec::MachineSpec;
    use crate::topology::StateTopology;
    use std::vec;

    // ── Construction ────────────────────────────────────────────────────────

    #[test]
    fn construction_is_silent() {
        let _m = StateMachine::<TSpec>::new(TCtx);
        let log = take_log();
        assert!(
            log.is_empty(),
            "construction must be silent, got: {:?}",
            log
        );
    }

    #[test]
    fn current_state_is_init_after_construction() {
        let m = StateMachine::<TSpec>::new(TCtx);
        assert!(m.current_state().is_init());
    }

    // ── Lifecycle command dispatch ──────────────────────────────────────────

    #[test]
    fn start_command_exits_init_and_enters_initial_state() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        take_log();
        let outcome = m.dispatch(TEvent::Lifecycle(LifecycleCommand::Start));
        assert_eq!(take_log(), vec!["Init:exit", "Top:entry", "A:entry"]);
        assert!(matches!(
            outcome,
            DispatchOutcome::Started(MachineState::State(TState::A))
        ));
    }

    #[test]
    fn domain_events_dropped_in_init() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        take_log();
        m.dispatch(TEvent::GoB);
        m.dispatch(TEvent::Unhandled);
        m.dispatch(TEvent::UnhandledDeep);
        assert!(
            take_log().is_empty(),
            "domain events in Init must be handled as stay"
        );
    }

    #[test]
    fn current_state_returns_leaf_after_start() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        m.dispatch(TEvent::Lifecycle(LifecycleCommand::Start));
        take_log();
        assert!(matches!(m.current_state(), MachineState::State(TState::A)));
    }

    // ── LCA transitions ─────────────────────────────────────────────────────

    #[test]
    fn lca_transition_only_touches_states_below_lca() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::GoB);
        assert_eq!(take_log(), vec!["A:exit", "B:entry"]);
    }

    #[test]
    fn cross_subtree_transition_fully_exits_source_and_enters_target() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::GoC);
        assert_eq!(
            take_log(),
            vec!["A:exit", "Top:exit", "Other:entry", "C:entry"]
        );
    }

    #[test]
    fn self_transition_exits_and_reenters_the_same_state() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::SelfLoop);
        assert_eq!(take_log(), vec!["A:exit", "A:entry"]);
    }

    #[test]
    fn stay_guard_produces_no_callbacks() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::NoOp);
        assert!(take_log().is_empty(), "Stay must produce no callbacks");
    }

    #[test]
    fn current_state_tracks_leaf_across_transitions() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        m.dispatch(TEvent::Lifecycle(LifecycleCommand::Start));
        take_log();
        assert!(matches!(m.current_state(), MachineState::State(TState::A)));

        m.dispatch(TEvent::GoB);
        take_log();
        assert!(matches!(m.current_state(), MachineState::State(TState::B)));

        let mut m2 = machine_in_a();
        m2.dispatch(TEvent::GoC);
        take_log();
        assert!(matches!(m2.current_state(), MachineState::State(TState::C)));
    }

    // ── Parent bubbling ─────────────────────────────────────────────────────

    #[test]
    fn unhandled_event_bubbles_to_parent() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::Unhandled);
        assert_eq!(take_log(), vec!["Top:handled_Unhandled"]);
    }

    #[test]
    fn unhandled_event_bubbles_to_root_rules() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::UnhandledDeep);
        assert_eq!(take_log(), vec!["root_on_event:UnhandledDeep"]);
    }

    // ── Guard::Reset ────────────────────────────────────────────────────────

    #[test]
    fn reset_from_shallow_state_exits_full_chain_then_calls_init_entry() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::Reset);
        assert_eq!(take_log(), vec!["A:exit", "Top:exit", "Init:entry"]);
        assert!(m.current_state().is_init());
    }

    #[test]
    fn reset_from_deep_state_exits_all_ancestors() {
        let mut m = machine_in_c();
        m.dispatch(TEvent::Reset);
        assert_eq!(take_log(), vec!["C:exit", "Other:exit", "Init:entry"]);
        assert!(m.current_state().is_init());
    }

    #[test]
    fn lifecycle_reset_command_works() {
        let mut m = machine_in_a();
        let outcome = m.dispatch(TEvent::Lifecycle(LifecycleCommand::Reset));
        assert!(matches!(outcome, DispatchOutcome::Reset));
        assert_eq!(take_log(), vec!["A:exit", "Top:exit", "Init:entry"]);
        assert!(matches!(outcome, DispatchOutcome::Reset));
        assert!(m.current_state().is_init());
    }

    // ── Lifecycle command dispatch from Init ─────────────────────────────────

    #[test]
    fn double_start_from_init_is_idempotent() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        m.dispatch(TEvent::Lifecycle(LifecycleCommand::Start));
        take_log();

        // Second start - machine is in operational state, lifecycle Start is handled
        let outcome = m.dispatch(TEvent::Lifecycle(LifecycleCommand::Start));
        assert!(matches!(outcome, DispatchOutcome::HandledNoTransition));
        assert!(
            take_log().is_empty(),
            "second start from operational must not fire callbacks"
        );
    }

    #[test]
    fn reset_from_init_is_noop() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        take_log();
        let outcome = m.dispatch(TEvent::Lifecycle(LifecycleCommand::Reset));
        assert!(matches!(outcome, DispatchOutcome::Reset));
        assert!(m.current_state().is_init());
    }

    // ── Topology invariants ─────────────────────────────────────────────────

    #[test]
    fn topology_has_no_cycles() {
        use std::collections::HashSet;
        let all_states = [TState::Top, TState::A, TState::B, TState::Other, TState::C];
        for &start in &all_states {
            let mut seen = HashSet::new();
            let mut cursor = Some(start);
            while let Some(s) = cursor {
                assert!(seen.insert(s), "cycle detected at {:?}", s);
                cursor = s.parent();
            }
        }
    }

    #[test]
    fn all_transition_targets_are_leaf_states() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        m.dispatch(TEvent::Lifecycle(LifecycleCommand::Start));
        take_log();
        assert!(m
            .current_state()
            .as_state()
            .map(|s| s.is_leaf())
            .unwrap_or(false));

        m.dispatch(TEvent::GoB);
        take_log();
        assert!(m
            .current_state()
            .as_state()
            .map(|s| s.is_leaf())
            .unwrap_or(false));

        let mut m2 = machine_in_a();
        m2.dispatch(TEvent::GoC);
        take_log();
        assert!(m2
            .current_state()
            .as_state()
            .map(|s| s.is_leaf())
            .unwrap_or(false));
    }

    // ── is_error default ────────────────────────────────────────────────────

    #[test]
    fn default_is_error_returns_false() {
        assert!(!TSpec::is_error(&TState::A));
        assert!(!TSpec::is_error(&TState::B));
        assert!(!TSpec::is_error(&TState::C));
        assert!(!TSpec::is_error(&TState::Top));
        assert!(!TSpec::is_error(&TState::Other));
    }

    // ── NoRuleMatched outcome ───────────────────────────────────────────────

    #[test]
    fn event_with_no_matching_rule_anywhere_is_unhandled() {
        let mut m = machine_in_c();
        take_log();
        let outcome = m.dispatch(TEvent::GoB);
        assert!(matches!(outcome, DispatchOutcome::NoRuleMatched));
        assert!(matches!(m.current_state(), MachineState::State(TState::C)));
        assert!(take_log().is_empty());
    }

    // ── ActionResult::Err path through guard ────────────────────────────────

    #[test]
    fn action_error_triggers_error_guard_branch() {
        let mut m = machine_in_a();
        take_log();
        let outcome = m.dispatch(TEvent::TriggerErr);
        assert!(matches!(m.current_state(), MachineState::State(TState::C)));
        assert!(matches!(outcome, DispatchOutcome::Transition(_)));
        assert!(take_log().contains(&"A:TriggerErr:action"));
    }

    // ── LeafState invariant ────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn leaf_state_new_with_composite_state_panics_in_debug() {
        use crate::topology::LeafState;
        let _ = LeafState::new(TState::Top);
    }

    // ── Lifecycle Stop command ──────────────────────────────────────────────

    #[test]
    fn stop_command_returns_stopped_outcome() {
        let mut m = machine_in_a();
        take_log();
        let outcome = m.dispatch(TEvent::Lifecycle(LifecycleCommand::Stop));
        assert!(matches!(outcome, DispatchOutcome::Stopped));
        assert!(m.current_state().is_init());
        assert!(take_log().contains(&"A:exit") || take_log().contains(&"Top:exit"));
    }

    #[test]
    fn ping_command_returns_alive_outcome() {
        let mut m = machine_in_a();
        take_log();
        let outcome = m.dispatch(TEvent::Lifecycle(LifecycleCommand::Ping));
        assert!(matches!(outcome, DispatchOutcome::Alive));
        assert!(take_log().is_empty());
    }
}

#[cfg(all(test, feature = "std"))]
mod lifecycle_dispatch;
