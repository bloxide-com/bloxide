//! HSM engine unit tests.
//!
//! Each test corresponds to one or more acceptance criteria from
//! `spec/architecture/02-hsm-engine.md`. Tests are grouped by the spec section
//! they cover so it is easy to trace a failing test back to the relevant
//! design decision.
//!
//! Run with: `cargo test -p bloxide-core --features std`
//!
//! # Testing Convention (SDD → TDD flow)
//!
//! 1. **Spec first** — acceptance criteria live in `spec/architecture/` or
//!    `spec/bloxes/<name>.md` before any test is written.
//! 2. **One test per criterion** — each `#[test]` fn is named and documented
//!    to match its criterion. If a test covers two criteria, split it.
//! 3. **Fixture isolation** — shared HSM topology lives in `fixture.rs`, not
//!    inline in test functions. This keeps tests readable and the topology
//!    reusable across test sub-modules.
//! 4. **No executor** — all tests use `TestRuntime` / direct `machine.dispatch()`
//!    so they run under `cargo test` with no Embassy or Tokio executor.
//! 5. **Sync after implementation** — if code reveals a spec error, update the
//!    spec first, then adjust the test, then fix the code.

#[cfg(feature = "std")]
pub(crate) mod fixture;

#[cfg(all(test, feature = "std"))]
mod hsm_engine {
    use super::fixture::*;
    use crate::engine::StateMachine;
    use crate::spec::MachineSpec;
    use crate::topology::StateTopology;
    use std::vec; // required: #![no_std] crate, vec! not in prelude

    // ── Construction (spec §"StateMachine construction") ─────────────────────

    /// AC: `StateMachine::new` is silent — no callbacks fire.
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

    /// AC: `current_state()` returns `None` while in Init.
    #[test]
    fn current_state_is_none_in_init() {
        let m = StateMachine::<TSpec>::new(TCtx);
        assert_eq!(m.current_state(), None);
    }

    // ── Init dispatch (spec §"Init Dispatch") ────────────────────────────────

    /// AC: Dispatching `Start` calls `on_init_exit`, then enters the full path
    /// to `initial_state()` in root-first order.
    #[test]
    fn start_exits_init_and_enters_initial_state() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        take_log();
        m.dispatch(TEvent::Start);
        assert_eq!(take_log(), vec!["Init:exit", "Top:entry", "A:entry"]);
    }

    /// AC: Non-Start events received while in Init are silently dropped.
    #[test]
    fn non_start_events_dropped_in_init() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        take_log();
        m.dispatch(TEvent::GoB);
        m.dispatch(TEvent::Unhandled);
        m.dispatch(TEvent::UnhandledDeep);
        assert!(
            take_log().is_empty(),
            "non-Start events in Init must be dropped"
        );
    }

    /// AC: `current_state()` reflects the leaf state after `Start`.
    #[test]
    fn current_state_returns_leaf_after_start() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        m.dispatch(TEvent::Start);
        take_log();
        assert_eq!(m.current_state(), Some(TState::A));
    }

    // ── LCA transitions (spec §"LCA Transition Algorithm") ───────────────────

    /// AC: Transition A → B (shared ancestor Top): only A:exit and B:entry fire;
    /// Top is neither exited nor re-entered.
    #[test]
    fn lca_transition_only_touches_states_below_lca() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::GoB);
        assert_eq!(take_log(), vec!["A:exit", "B:entry"]);
    }

    /// AC: Cross-subtree transition A → C (LCA = None): exits entire source
    /// chain leaf-first, then enters entire target chain root-first.
    #[test]
    fn cross_subtree_transition_fully_exits_source_and_enters_target() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::GoC);
        assert_eq!(
            take_log(),
            vec!["A:exit", "Top:exit", "Other:entry", "C:entry"]
        );
    }

    /// AC: Self-transition A → A: A:exit fires then A:entry (external self-transition,
    /// not a `Guard::Stay`).
    #[test]
    fn self_transition_exits_and_reenters_the_same_state() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::SelfLoop);
        assert_eq!(take_log(), vec!["A:exit", "A:entry"]);
    }

    /// AC: `Guard::Stay` produces no on_exit or on_entry callbacks.
    #[test]
    fn stay_guard_produces_no_callbacks() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::NoOp);
        assert!(take_log().is_empty(), "Stay must produce no callbacks");
    }

    /// AC: `current_state()` tracks the active leaf across multiple transitions.
    #[test]
    fn current_state_tracks_leaf_across_transitions() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        take_log();
        m.dispatch(TEvent::Start);
        take_log();
        assert_eq!(m.current_state(), Some(TState::A));

        m.dispatch(TEvent::GoB);
        take_log();
        assert_eq!(m.current_state(), Some(TState::B));

        let mut m2 = machine_in_a();
        m2.dispatch(TEvent::GoC);
        take_log();
        assert_eq!(m2.current_state(), Some(TState::C));
    }

    // ── Parent bubbling (spec §"Operational Dispatch Algorithm") ─────────────

    /// AC: An event unhandled by the leaf state bubbles one level to the parent.
    #[test]
    fn unhandled_event_bubbles_to_parent() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::Unhandled);
        assert_eq!(take_log(), vec!["Top:handled_Unhandled"]);
    }

    /// AC: An event unhandled by all states bubbles all the way to root rules.
    #[test]
    fn unhandled_event_bubbles_to_root_rules() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::UnhandledDeep);
        assert_eq!(take_log(), vec!["root_on_event:UnhandledDeep"]);
    }

    // ── Reset / Terminate (spec §"Terminate Semantics") ──────────────────────

    /// AC: `Guard::Reset` exits the full operational chain leaf-first,
    /// then calls `on_init_entry`.
    #[test]
    fn reset_from_shallow_state_exits_full_chain_then_calls_init_entry() {
        let mut m = machine_in_a();
        m.dispatch(TEvent::Reset);
        // A is under Top. Reset exits A → Top, then on_init_entry fires.
        assert_eq!(take_log(), vec!["A:exit", "Top:exit", "Init:entry"]);
    }

    /// AC: Reset from a deeply nested state exits every intermediate ancestor.
    #[test]
    fn reset_from_deep_state_exits_all_ancestors() {
        let mut m = machine_in_c();
        m.dispatch(TEvent::Reset);
        // C is under Other. Reset exits C → Other, then on_init_entry fires.
        assert_eq!(take_log(), vec!["C:exit", "Other:exit", "Init:entry"]);
    }

    // ── start() method (spec §"StateMachine::start") ─────────────────────────

    /// AC: `start()` called on a machine in Init fires `on_init_exit` then
    /// enters the full path to `initial_state()` — same sequence as dispatching
    /// a Start event.
    #[test]
    fn start_method_fires_same_sequence_as_dispatch_start() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        take_log();
        let outcome = m.start();
        assert_eq!(outcome, crate::engine::DispatchOutcome::Started(TState::A));
        assert_eq!(take_log(), vec!["Init:exit", "Top:entry", "A:entry"]);
    }

    /// AC: `start()` called on an already-operational machine returns Stay
    /// and fires no callbacks.
    #[test]
    fn double_start_is_idempotent() {
        let mut m = machine_in_a();
        take_log();
        let outcome = m.start();
        assert_eq!(outcome, crate::engine::DispatchOutcome::Stay);
        assert!(
            take_log().is_empty(),
            "second start() must fire no callbacks"
        );
    }

    // ── reset() method (spec §"Terminate Semantics") ──────────────────────────

    /// AC: `reset()` called on a machine already in Init returns AlreadyInit and
    /// fires no callbacks.
    #[test]
    fn double_reset_is_idempotent() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        take_log();
        let outcome = m.reset();
        assert_eq!(outcome, crate::engine::DispatchOutcome::AlreadyInit);
        assert!(
            take_log().is_empty(),
            "reset() in Init must fire no callbacks"
        );
    }

    // ── Topology invariants (spec §"Topology Invariants") ────────────────────

    /// AC: `parent()` forms a strict tree — no cycles exist in the ancestry graph.
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

    /// AC: Every transition target satisfies `is_leaf` (composite states are
    /// never valid transition destinations).
    #[test]
    fn all_transition_targets_are_leaf_states() {
        let mut m = StateMachine::<TSpec>::new(TCtx);
        take_log();
        m.dispatch(TEvent::Start);
        take_log();
        assert!(m.current_state().unwrap().is_leaf());

        m.dispatch(TEvent::GoB);
        take_log();
        assert!(m.current_state().unwrap().is_leaf());

        let mut m2 = machine_in_a();
        m2.dispatch(TEvent::GoC);
        take_log();
        assert!(m2.current_state().unwrap().is_leaf());
    }

    // ── is_error default (spec §"Supervision") ───────────────────────────────

    /// AC: Default `is_error` returns false for all states.
    #[test]
    fn default_is_error_returns_false() {
        assert!(!TSpec::is_error(&TState::A));
        assert!(!TSpec::is_error(&TState::B));
        assert!(!TSpec::is_error(&TState::C));
        assert!(!TSpec::is_error(&TState::Top));
        assert!(!TSpec::is_error(&TState::Other));
    }

    // ── Unhandled outcome (spec §"Operational Dispatch Algorithm") ────────────

    /// AC: An event that matches no rule in any state (nor root rules) returns
    /// `DispatchOutcome::Unhandled`. The machine state is unchanged.
    #[test]
    fn event_with_no_matching_rule_anywhere_is_unhandled() {
        use crate::engine::DispatchOutcome;
        // C has no transitions and Other (its parent) has no transitions.
        // GoB is also unhandled by C/Other hierarchy.
        let mut m = machine_in_c();
        take_log();
        let outcome = m.dispatch(TEvent::GoB);
        assert_eq!(
            outcome,
            DispatchOutcome::Unhandled,
            "event with no matching rule must return Unhandled"
        );
        assert_eq!(
            m.current_state(),
            Some(TState::C),
            "machine state must be unchanged after Unhandled"
        );
        assert!(
            take_log().is_empty(),
            "no callbacks must fire for Unhandled"
        );
    }

    // ── ActionResult::Err path through guard ─────────────────────────────────

    /// AC: When an action returns `ActionResult::Err`, `results.any_failed()` is
    /// true in the guard, allowing the guard to route to an error state.
    #[test]
    fn action_error_triggers_error_guard_branch() {
        use crate::engine::DispatchOutcome;
        let mut m = machine_in_a();
        take_log();
        // TriggerErr action returns ActionResult::Err; guard checks any_failed() → C
        let outcome = m.dispatch(TEvent::TriggerErr);
        assert_eq!(
            m.current_state(),
            Some(TState::C),
            "action error must route to the error branch (TState::C)"
        );
        assert!(
            matches!(outcome, DispatchOutcome::Transition(TState::C)),
            "outcome must be Transition(C)"
        );
        // The action fires, then the exit/entry sequence happens
        assert!(
            take_log().contains(&"A:TriggerErr:action"),
            "action must have fired"
        );
    }

    // ── LeafState invariant (spec §"Topology Invariants") ────────────────────

    /// AC: `LeafState::new` panics in debug mode when given a composite state.
    #[test]
    #[should_panic]
    fn leaf_state_new_with_composite_state_panics_in_debug() {
        use crate::topology::LeafState;
        // Top is composite (not is_leaf). debug_assert! in LeafState::new should fire.
        let _ = LeafState::new(TState::Top);
    }
}
