// Copyright 2025 Bloxide, all rights reserved
//! Samek QHsmTst topology proof suite (issue #136).
//!
//! Verifies the engine's entry/exit ordering, LCA computation, self-transitions,
//! cross-subtree transitions, and lifecycle handling against the GENERATED
//! topology (`BhsmTstState` from `blox.toml`). A recording spec mirrors the
//! blox's transition rules with trace-recording entry/exit actions — the
//! engine drives the real generated state paths, and the exact chain order
//! is asserted for every transition shape.
//!
//! LCA semantics (spec 01): exit `source_path[i+1..]` leaf-first, enter
//! `target_path[i+1..]` root-first; the LCA state itself never exits or
//! re-enters. `LCA = None` exits/enters the full chains. Self-transitions
//! exit and re-enter only the leaf.

use crate::{BhsmTstCtx, BhsmTstEvent, BhsmTstState};
use bhsm_tst_messages::{BhsmTstMsg, A, B, C, D, E, F, G, H, I, K, X};
use bloxide_core::engine::{DispatchOutcome, MachineState, StateMachine};
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_core::mailboxes::NoMailboxes;
use bloxide_core::messaging::Envelope;
use bloxide_core::spec::{MachineSpec, StateFns};
use bloxide_core::topology::LeafState;
use bloxide_core::transition::{Decision, StateRule};

use std::cell::RefCell;
use std::thread_local;
use std::vec;
use std::vec::Vec;

thread_local! {
    static LOG: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn log(msg: &'static str) {
    LOG.with(|l| l.borrow_mut().push(msg));
}

fn take_log() -> Vec<&'static str> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

// ── Recording spec over the generated topology ─────────────────────────────
//
// Every action closure also calls `blox_ctx_noop::noop()` — the function the
// system-level codegen wires for all 17 declared actions of this blox
// (`crate = "blox_ctx_noop"`, `fn_name = "noop"`, `returns = "ActionResult"`).
// Entry/exit closures match the generated `|ctx| { ::blox_ctx_noop::noop(); }`
// shape (return discarded); transition actions match the generated bare-call
// shape (`returns = "ActionResult"` skips the normalization wrapper). This
// compiles and exercises the crate-ified actions through
// the engine, so a stale action declaration can never silently rot.

struct RecSpec;

impl MachineSpec for RecSpec {
    type State = BhsmTstState;
    type Event = BhsmTstEvent;
    type Ctx = BhsmTstCtx;
    type Mailboxes<R: bloxide_core::capability::BloxRuntime> = NoMailboxes;

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = &[
        &S_FNS, &S1_FNS, &S11_FNS, &S2_FNS, &S21_FNS, &S211_FNS, &ERROR_FNS,
    ];

    fn initial_state() -> BhsmTstState {
        BhsmTstState::S11
    }

    fn is_error(state: &BhsmTstState) -> bool {
        matches!(state, BhsmTstState::Error)
    }

    fn on_init_entry(_ctx: &mut BhsmTstCtx) {
        log("Init:entry");
    }

    fn on_init_exit(_ctx: &mut BhsmTstCtx) {
        log("Init:exit");
    }
}

macro_rules! rule {
    ($variant:ident, $guard:expr) => {
        StateRule {
            event_tag: BhsmTstEvent::MSG_TAG,
            matches: |ev| matches!(ev, BhsmTstEvent::Msg(Envelope(_, BhsmTstMsg::$variant(_)))),
            actions: &[],
            guard: $guard,
        }
    };
    ($variant:ident, $action:expr, $guard:expr) => {
        StateRule {
            event_tag: BhsmTstEvent::MSG_TAG,
            matches: |ev| matches!(ev, BhsmTstEvent::Msg(Envelope(_, BhsmTstMsg::$variant(_)))),
            actions: &[$action],
            guard: $guard,
        }
    };
}

static S_FNS: StateFns<RecSpec> = StateFns {
    on_entry: &[|_| {
        blox_ctx_noop::noop();
        log("s-ENTRY")
    }],
    on_exit: &[|_| {
        blox_ctx_noop::noop();
        log("s-EXIT")
    }],
    transitions: &[
        rule!(H, |_, _, _| Decision::Transition(LeafState::new(
            BhsmTstState::S11
        ))),
        rule!(
            I,
            |_, _| {
                log("s-I");
                blox_ctx_noop::noop()
            },
            |_, _, _| Decision::Stay
        ),
        rule!(K, |_, _, _| Decision::Transition(LeafState::new(
            BhsmTstState::Error
        ))),
        rule!(X, |_, _, _| Decision::Stop),
    ],
};

static S1_FNS: StateFns<RecSpec> = StateFns {
    on_entry: &[|_| {
        blox_ctx_noop::noop();
        log("s1-ENTRY")
    }],
    on_exit: &[|_| {
        blox_ctx_noop::noop();
        log("s1-EXIT")
    }],
    transitions: &[rule!(C, |_, _, _| Decision::Transition(LeafState::new(
        BhsmTstState::S211
    )))],
};

static S11_FNS: StateFns<RecSpec> = StateFns {
    on_entry: &[|_| {
        blox_ctx_noop::noop();
        log("s11-ENTRY")
    }],
    on_exit: &[|_| {
        blox_ctx_noop::noop();
        log("s11-EXIT")
    }],
    transitions: &[
        rule!(
            A,
            |_, _| {
                log("s11-A");
                blox_ctx_noop::noop()
            },
            |_, _, _| Decision::Transition(LeafState::new(BhsmTstState::S11))
        ),
        rule!(
            B,
            |_, _| {
                log("s11-B");
                blox_ctx_noop::noop()
            },
            |_, _, _| Decision::Transition(LeafState::new(BhsmTstState::S11))
        ),
        rule!(D, |_, _, _| Decision::Transition(LeafState::new(
            BhsmTstState::S211
        ))),
    ],
};

static S2_FNS: StateFns<RecSpec> = StateFns {
    on_entry: &[|_| {
        blox_ctx_noop::noop();
        log("s2-ENTRY")
    }],
    on_exit: &[|_| {
        blox_ctx_noop::noop();
        log("s2-EXIT")
    }],
    transitions: &[],
};

static S21_FNS: StateFns<RecSpec> = StateFns {
    on_entry: &[|_| {
        blox_ctx_noop::noop();
        log("s21-ENTRY")
    }],
    on_exit: &[|_| {
        blox_ctx_noop::noop();
        log("s21-EXIT")
    }],
    transitions: &[
        rule!(E, |_, _, _| Decision::Transition(LeafState::new(
            BhsmTstState::S211
        ))),
        rule!(G, |_, _, _| Decision::Transition(LeafState::new(
            BhsmTstState::S11
        ))),
    ],
};

static S211_FNS: StateFns<RecSpec> = StateFns {
    on_entry: &[|_| {
        blox_ctx_noop::noop();
        log("s211-ENTRY")
    }],
    on_exit: &[|_| {
        blox_ctx_noop::noop();
        log("s211-EXIT")
    }],
    transitions: &[rule!(F, |_, _, _| Decision::Transition(LeafState::new(
        BhsmTstState::S11
    )))],
};

static ERROR_FNS: StateFns<RecSpec> = StateFns {
    on_entry: &[|_| {
        blox_ctx_noop::noop();
        log("error-ENTRY")
    }],
    on_exit: &[|_| {
        blox_ctx_noop::noop();
        log("error-EXIT")
    }],
    transitions: &[],
};

// ── Harness ────────────────────────────────────────────────────────────────

fn msg(v: BhsmTstMsg) -> BhsmTstEvent {
    BhsmTstEvent::Msg(Envelope(0, v))
}

fn machine_in_s11() -> StateMachine<RecSpec> {
    let mut m = StateMachine::<RecSpec>::new(BhsmTstCtx::new(1));
    m.dispatch(BhsmTstEvent::Lifecycle(LifecycleCommand::Start));
    take_log();
    m
}

fn machine_in_s211() -> StateMachine<RecSpec> {
    let mut m = machine_in_s11();
    m.dispatch(msg(BhsmTstMsg::D(D)));
    take_log();
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_enters_s11_through_entry_chain() {
        let mut m = StateMachine::<RecSpec>::new(BhsmTstCtx::new(1));
        let outcome = m.dispatch(BhsmTstEvent::Lifecycle(LifecycleCommand::Start));
        assert_eq!(
            take_log(),
            vec!["Init:exit", "s-ENTRY", "s1-ENTRY", "s11-ENTRY"]
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::Started(MachineState::State(BhsmTstState::S11))
        ));
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S11)
        ));
    }

    #[test]
    fn a_in_s11_self_transitions() {
        let mut m = machine_in_s11();
        m.dispatch(msg(BhsmTstMsg::A(A)));
        // Action first (actions before guards), then exit/enter only the leaf.
        assert_eq!(take_log(), vec!["s11-A", "s11-EXIT", "s11-ENTRY"]);
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S11)
        ));
    }

    #[test]
    fn b_in_s11_self_transitions() {
        let mut m = machine_in_s11();
        m.dispatch(msg(BhsmTstMsg::B(B)));
        assert_eq!(take_log(), vec!["s11-B", "s11-EXIT", "s11-ENTRY"]);
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S11)
        ));
    }

    #[test]
    fn d_in_s11_cross_subtree_lca_s() {
        let mut m = machine_in_s11();
        m.dispatch(msg(BhsmTstMsg::D(D)));
        // LCA(S11, S211) = S: exit below S (S11, S1), enter below S (S2, S21, S211).
        // S itself does NOT exit or re-enter (spec 01).
        assert_eq!(
            take_log(),
            vec!["s11-EXIT", "s1-EXIT", "s2-ENTRY", "s21-ENTRY", "s211-ENTRY"]
        );
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S211)
        ));
    }

    #[test]
    fn c_in_s11_bubbles_to_s1() {
        let mut m = machine_in_s11();
        m.dispatch(msg(BhsmTstMsg::C(C)));
        // C is handled at S1 (bubbled); same LCA=S chain as D.
        assert_eq!(
            take_log(),
            vec!["s11-EXIT", "s1-EXIT", "s2-ENTRY", "s21-ENTRY", "s211-ENTRY"]
        );
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S211)
        ));
    }

    #[test]
    fn e_in_s211_bubbles_to_s21_parent_child() {
        let mut m = machine_in_s211();
        m.dispatch(msg(BhsmTstMsg::E(E)));
        // Self-transition on S211 handled at S21: exit/enter only the leaf.
        assert_eq!(take_log(), vec!["s211-EXIT", "s211-ENTRY"]);
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S211)
        ));
    }

    #[test]
    fn f_in_s211_cross_back_lca_s() {
        let mut m = machine_in_s211();
        m.dispatch(msg(BhsmTstMsg::F(F)));
        // LCA(S211, S11) = S: exit S211,S21,S2; enter S1,S11.
        assert_eq!(
            take_log(),
            vec!["s211-EXIT", "s21-EXIT", "s2-EXIT", "s1-ENTRY", "s11-ENTRY"]
        );
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S11)
        ));
    }

    #[test]
    fn g_in_s211_bubbles_to_s21_lca_s() {
        let mut m = machine_in_s211();
        m.dispatch(msg(BhsmTstMsg::G(G)));
        // Same LCA=S chain as F (handled at S21).
        assert_eq!(
            take_log(),
            vec!["s211-EXIT", "s21-EXIT", "s2-EXIT", "s1-ENTRY", "s11-ENTRY"]
        );
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S11)
        ));
    }

    #[test]
    fn h_from_deep_state_resets_to_s11() {
        let mut m = machine_in_s211();
        m.dispatch(msg(BhsmTstMsg::H(H)));
        // Handled at S; LCA=S: exit up to (not incl.) S, enter S1,S11.
        assert_eq!(
            take_log(),
            vec!["s211-EXIT", "s21-EXIT", "s2-EXIT", "s1-ENTRY", "s11-ENTRY"]
        );
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S11)
        ));
    }

    #[test]
    fn i_at_top_level_is_absorbed() {
        let mut m = machine_in_s11();
        let outcome = m.dispatch(msg(BhsmTstMsg::I(I)));
        // Stay: only the action fires — no exit/entry anywhere.
        assert_eq!(take_log(), vec!["s-I"]);
        assert_eq!(outcome, DispatchOutcome::HandledNoTransition);
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S11)
        ));
    }

    #[test]
    fn k_from_any_state_transitions_to_error() {
        let mut m = machine_in_s11();
        let outcome = m.dispatch(msg(BhsmTstMsg::K(K)));
        // Error is top-level (LCA = None): full exit chain, then error entry.
        assert_eq!(
            take_log(),
            vec!["s11-EXIT", "s1-EXIT", "s-EXIT", "error-ENTRY"]
        );
        assert!(matches!(outcome, DispatchOutcome::Failed));
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::Error)
        ));
    }

    #[test]
    fn x_from_any_state_guard_stop_suspends() {
        let mut m = machine_in_s11();
        let outcome = m.dispatch(msg(BhsmTstMsg::X(X)));
        // Decision::Stop: full exit chain + on_init_entry; suspended in Init.
        assert_eq!(
            take_log(),
            vec!["s11-EXIT", "s1-EXIT", "s-EXIT", "Init:entry"]
        );
        assert_eq!(outcome, DispatchOutcome::Stopped);
        assert!(matches!(m.current_state(), MachineState::Init));
    }

    #[test]
    fn lifecycle_reset_goes_directly_to_initial_state() {
        let mut m = machine_in_s211();
        let outcome = m.dispatch(BhsmTstEvent::Lifecycle(LifecycleCommand::Reset));
        // Reset skips Init (no Init:entry): LCA=S chain to S11.
        assert_eq!(
            take_log(),
            vec!["s211-EXIT", "s21-EXIT", "s2-EXIT", "s1-ENTRY", "s11-ENTRY"]
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::Started(MachineState::State(BhsmTstState::S11))
        ));
    }

    #[test]
    fn lifecycle_stop_suspends_to_init() {
        let mut m = machine_in_s11();
        let outcome = m.dispatch(BhsmTstEvent::Lifecycle(LifecycleCommand::Stop));
        assert_eq!(
            take_log(),
            vec!["s11-EXIT", "s1-EXIT", "s-EXIT", "Init:entry"]
        );
        assert_eq!(outcome, DispatchOutcome::Stopped);
        assert!(matches!(m.current_state(), MachineState::Init));
    }

    #[test]
    fn unhandled_event_bubbles_and_is_dropped() {
        let mut m = machine_in_s211();
        // A is handled only in S11 — from S211 it bubbles S211→S21→S2→S→root
        // with no matching rule anywhere.
        let outcome = m.dispatch(msg(BhsmTstMsg::A(A)));
        assert_eq!(outcome, DispatchOutcome::NoRuleMatched);
        assert!(take_log().is_empty());
        assert!(matches!(
            m.current_state(),
            MachineState::State(BhsmTstState::S211)
        ));
    }
}
