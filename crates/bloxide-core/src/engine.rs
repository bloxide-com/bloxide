// Copyright 2025 Bloxide, all rights reserved
// ── Imports ──────────────────────────────────────────────────────────────────

use crate::event_tag::{EventTag, WILDCARD_TAG};
use crate::spec::{MachineSpec, StateFns};
use crate::topology::StateTopology;
use crate::transition::{ActionFn, ActionResults, Guard, TransitionRule};

// ── Handler-table bounds-checked lookup ──────────────────────────────────────

#[inline(always)]
fn handler_fns<S: MachineSpec>(state: &S::State) -> &'static StateFns<S> {
    let idx = state.as_index();
    debug_assert!(
        idx < S::HANDLER_TABLE.len(),
        "state index {} out of bounds for HANDLER_TABLE (len {})",
        idx,
        S::HANDLER_TABLE.len()
    );
    S::HANDLER_TABLE[idx]
}

// ── MachinePhase ──────────────────────────────────────────────────────────────

/// Tracks which phase of its lifecycle the state machine is in.
///
/// - `Init` — the machine is waiting for a Start event. Non-Start events are
///   silently dropped. `on_init_entry` fires when entering this phase after a
///   Reset; construction is silent (Init is entered without any callbacks).
/// - `Operational(state)` — the machine is running in a user-declared leaf state.
///
/// This enum replaces the previous `current_state: S::State + in_init: bool`
/// pair, making it impossible to accidentally read the state while in Init.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachinePhase<State> {
    Init,
    Operational(State),
}

// ── LCA helper ────────────────────────────────────────────────────────────────

/// Find the index of the deepest common ancestor in two root-first paths.
///
/// Returns `Some(i)` where `i` is the index of the last common entry, or
/// `None` if the paths share no common prefix (the states are in different
/// top-level subtrees with no user-declared common ancestor).
///
/// # Precondition — tree topology
///
/// `parent()` must form a **tree**: every two root-first paths either share a
/// monotone common prefix and then diverge, or share no prefix at all. If the
/// topology is a DAG (paths re-converge after diverging), the result is
/// incorrect. The `debug_assert!` below catches this in debug builds.
fn find_lca<S: MachineSpec>(source_path: &[S::State], target_path: &[S::State]) -> Option<usize> {
    let len = source_path.len().min(target_path.len());
    let mut lca = None;
    for i in 0..len {
        if source_path[i] == target_path[i] {
            lca = Some(i);
        } else {
            // Tree invariant: once paths diverge they must not re-converge.
            // A re-convergence indicates a DAG topology, which breaks the
            // prefix-based LCA algorithm.
            #[cfg(debug_assertions)]
            for j in (i + 1)..len {
                debug_assert!(
                    source_path[j] != target_path[j],
                    "state topology is not a tree: paths re-converge at index {}",
                    j
                );
            }
            break;
        }
    }
    lca
}

// ── Action runner ─────────────────────────────────────────────────────────────

/// Run all actions in the slice, collecting their results.
/// Fast path: returns `ActionResults::new()` immediately if the slice is empty,
/// avoiding the `FromIterator` overhead that would otherwise be incurred.
#[inline]
fn run_actions<S: MachineSpec>(
    actions: &[ActionFn<S>],
    ctx: &mut S::Ctx,
    event: &S::Event,
) -> ActionResults {
    if actions.is_empty() {
        ActionResults::new()
    } else {
        actions.iter().map(|f| f(ctx, event)).collect()
    }
}

// ── Rule evaluator ────────────────────────────────────────────────────────────

/// Evaluate a slice of `TransitionRule<S, G>` against the current event.
///
/// Iterates rules in order; applies the event-tag fast-reject and the
/// `matches` predicate. For the first matching rule, runs all actions and
/// calls the guard. Returns `Some(guard_outcome)` on the first match, or
/// `None` if no rule matches.
///
/// The borrow ordering — `actions` receives `&mut ctx`, `guard` receives
/// `&ctx` — is preserved: the mutable reborrow ends when `run_actions`
/// returns, after which `ctx` is borrowed immutably for the guard call.
#[inline]
fn eval_rules<S: MachineSpec, G>(
    rules: &[TransitionRule<S, G>],
    ctx: &mut S::Ctx,
    event: &S::Event,
    event_tag: u8,
) -> Option<G> {
    for rule in rules {
        // Fast reject: skip rules whose event_tag doesn't match.
        // WILDCARD_TAG (255) is the sentinel — those rules always proceed.
        if rule.event_tag != WILDCARD_TAG && rule.event_tag != event_tag {
            continue;
        }
        if (rule.matches)(event) {
            let results = run_actions::<S>(rule.actions, ctx, event);
            return Some((rule.guard)(ctx, &results, event));
        }
    }
    None
}

// ── DispatchOutcome ───────────────────────────────────────────────────────────

/// The outcome of dispatching an event to a state machine.
/// Returned by `dispatch()`, `start()`, and `reset()` so the runtime
/// can observe lifecycle transitions without coupling to actor event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome<State> {
    /// Non-Start event received while in Init — event blocked by design.
    /// The machine remains in Init waiting for a Start event.
    NotStarted,
    /// Event dispatched to an operational machine; no rule matched anywhere
    /// (implicit Stay — machine state is unchanged).
    Unhandled,
    /// `reset()` called while already in Init — no-op idempotency case.
    AlreadyInit,
    /// Machine exited Init and entered initial_state(). Carries the
    /// initial leaf state.
    Started(State),
    /// Event handled; machine stays in its current state.
    Stay,
    /// Event handled; machine transitioned to a new leaf state.
    Transition(State),
    /// A guard returned Guard::Reset — machine exited all operational
    /// states and re-entered engine-implicit Init.
    Reset,
}

// ── StateMachine ─────────────────────────────────────────────────────────────

/// A hierarchical state machine.
///
/// Manages both the engine-implicit Init phase and the user-declared operational
/// state hierarchy. The machine starts in Init **silently** (no callbacks fire
/// on initial construction). It moves to `initial_state()` when `is_start`
/// returns `true` for a dispatched event. All events received while in Init
/// that do not satisfy `is_start` are silently dropped.
///
/// `on_init_entry` fires only when the machine is **reset** (via
/// `Guard::Reset`), not on first construction. This allows it to serve
/// as a pure post-reset notification (e.g. "I am ready to be restarted")
/// without firing prematurely during system wiring.
pub struct StateMachine<S: MachineSpec> {
    /// Current machine phase. `Init` while waiting for Start;
    /// `Operational(state)` while running in user-declared states.
    phase: MachinePhase<S::State>,
    ctx: S::Ctx,
}

impl<S: MachineSpec> StateMachine<S> {
    /// Construct a new machine in engine-implicit Init.
    ///
    /// Init is entered **silently** — `on_init_entry` does NOT fire here. It
    /// only fires after a reset (`Guard::Reset`). The machine stays in
    /// Init until an event satisfying `S::is_start` is dispatched.
    pub fn new(ctx: S::Ctx) -> Self {
        StateMachine {
            phase: MachinePhase::Init,
            ctx,
        }
    }

    /// Dispatch an event to the machine (run-to-completion).
    ///
    /// While in Init, only events satisfying `S::is_start` are acted on:
    /// the engine calls `on_init_exit`, then enters the `initial_state()` path.
    /// All other events are silently dropped while in Init.
    pub fn dispatch(&mut self, event: S::Event) -> DispatchOutcome<S::State> {
        match self.phase {
            MachinePhase::Init => {
                if S::is_start(&event) {
                    self.leave_init()
                } else {
                    trace_init_drop_event!(event);
                    DispatchOutcome::NotStarted
                }
            }
            MachinePhase::Operational(_) => self.process_event(event),
        }
    }

    /// Transition from engine-implicit Init to initial_state().
    ///
    /// Called directly by the runtime to start an actor without needing to
    /// construct a LifecycleMsg event. If the machine is already operational
    /// (e.g. called twice), returns DispatchOutcome::Stay with no side effects.
    pub fn start(&mut self) -> DispatchOutcome<S::State> {
        match self.phase {
            MachinePhase::Init => self.leave_init(),
            MachinePhase::Operational(_) => DispatchOutcome::Stay,
        }
    }

    /// Exit Init and enter the initial operational state.
    ///
    /// Fires `on_init_exit`, enters the full path to `initial_state()` in
    /// root-first order (calling `on_entry` for each state), and sets the
    /// phase to `Operational(initial)`.
    fn leave_init(&mut self) -> DispatchOutcome<S::State> {
        trace_init_exit!();
        S::on_init_exit(&mut self.ctx);
        let initial = S::initial_state();
        let path = initial.path();
        for &state in path.iter() {
            trace_on_entry!(state);
            for action in handler_fns::<S>(&state).on_entry {
                action(&mut self.ctx);
            }
        }
        self.phase = MachinePhase::Operational(initial);
        DispatchOutcome::Started(initial)
    }

    /// Exit all operational states and re-enter engine-implicit Init.
    ///
    /// Called directly by the runtime when it needs to reset an actor. All
    /// on_exit handlers fire from the current leaf up to the topmost ancestor,
    /// then on_init_entry fires.
    /// If already in Init (e.g. called twice), returns DispatchOutcome::AlreadyInit.
    pub fn reset(&mut self) -> DispatchOutcome<S::State> {
        match self.phase {
            MachinePhase::Operational(_) => {
                self.enter_init();
                DispatchOutcome::Reset
            }
            MachinePhase::Init => DispatchOutcome::AlreadyInit,
        }
    }

    /// Shared reference to the machine's context.
    pub fn ctx(&self) -> &S::Ctx {
        &self.ctx
    }

    /// Mutable reference to the machine's context.
    pub fn ctx_mut(&mut self) -> &mut S::Ctx {
        &mut self.ctx
    }

    /// Returns the current operational leaf state, or `None` if the machine
    /// is in engine-implicit Init (waiting for a Start event).
    pub fn current_state(&self) -> Option<S::State> {
        match self.phase {
            MachinePhase::Init => None,
            MachinePhase::Operational(s) => Some(s),
        }
    }

    fn process_event(&mut self, event: S::Event) -> DispatchOutcome<S::State> {
        let current = match self.phase {
            MachinePhase::Operational(s) => s,
            MachinePhase::Init => unreachable!("process_event called while in Init"),
        };

        // Walk from the current leaf state up through its ancestors. For each
        // state, evaluate its transition rules in order. The first matching
        // rule wins: iterate actions (collecting ActionResults), then evaluate
        // the guard. Bubbling to the parent is implicit — it happens when no
        // rule in the current state matches. We walk the precomputed root-first
        // path in reverse (leaf → root) so child rules take priority over
        // parent rules.
        let event_tag = event.event_tag();
        let current_path = current.path();

        for &ancestor in current_path.iter().rev() {
            let fns = handler_fns::<S>(&ancestor);
            trace_on_event_received!(ancestor, event);

            if let Some(outcome) =
                eval_rules::<S, Guard<S>>(fns.transitions, &mut self.ctx, &event, event_tag)
            {
                match outcome {
                    Guard::Transition(leaf) => {
                        let target = leaf.into_inner();
                        self.change_state(target);
                        return DispatchOutcome::Transition(target);
                    }
                    Guard::Stay => return DispatchOutcome::Stay,
                    Guard::Reset => {
                        self.enter_init();
                        return DispatchOutcome::Reset;
                    }
                }
            }
        }

        // Event bubbled past all user-declared states — evaluate root rules.
        if let Some(outcome) =
            eval_rules::<S, Guard<S>>(S::root_transitions(), &mut self.ctx, &event, event_tag)
        {
            match outcome {
                Guard::Transition(leaf) => {
                    let target = leaf.into_inner();
                    self.change_state(target);
                    return DispatchOutcome::Transition(target);
                }
                Guard::Stay => return DispatchOutcome::Stay,
                Guard::Reset => {
                    self.enter_init();
                    return DispatchOutcome::Reset;
                }
            }
        }
        // No rule matched anywhere — event unhandled (equivalent to implicit Stay).
        DispatchOutcome::Unhandled
    }

    /// Exit the entire operational state chain and re-enter engine-implicit Init.
    ///
    /// Called when a `Guard::Reset` is returned from a root rule, or directly
    /// via `machine.reset()`. All `on_exit` handlers fire from the current leaf
    /// up to the topmost ancestor. `on_exit` handlers must be safe to call from
    /// any operational state at any time.
    fn enter_init(&mut self) {
        let current = match self.phase {
            MachinePhase::Operational(s) => s,
            MachinePhase::Init => unreachable!("enter_init called while already in Init"),
        };
        let source_path = current.path();
        for &state in source_path.iter().rev() {
            trace_on_exit!(state);
            for action in handler_fns::<S>(&state).on_exit {
                action(&mut self.ctx);
            }
        }
        trace_init_entry!();
        S::on_init_entry(&mut self.ctx);
        self.phase = MachinePhase::Init;
    }

    fn change_state(&mut self, target: S::State) {
        let source = match self.phase {
            MachinePhase::Operational(s) => s,
            MachinePhase::Init => unreachable!("change_state called while in Init"),
        };

        let source_path = source.path();
        let target_path = target.path();

        // For a self-transition, the LCA is forced to the virtual parent of the
        // current state. If the state is top-level (no user parent), LCA = None,
        // causing full exit + re-entry.
        let lca = if source == target {
            if source_path.len() >= 2 {
                Some(source_path.len() - 2)
            } else {
                None // top-level self-transition: exit and re-enter
            }
        } else {
            find_lca::<S>(source_path, target_path)
        };

        trace_on_transition!(source, target, lca.map(|i| source_path[i]));

        match lca {
            Some(lca_index) => {
                // Exit from source leaf up to (but not including) the LCA.
                for &state in source_path[lca_index + 1..].iter().rev() {
                    trace_on_exit!(state);
                    for action in handler_fns::<S>(&state).on_exit {
                        action(&mut self.ctx);
                    }
                }
                // Enter from the child of LCA down to (and including) the target.
                for &state in target_path[lca_index + 1..].iter() {
                    trace_on_entry!(state);
                    for action in handler_fns::<S>(&state).on_entry {
                        action(&mut self.ctx);
                    }
                }
            }
            None => {
                // No common user ancestor: exit entire source chain, enter entire target chain.
                for &state in source_path.iter().rev() {
                    trace_on_exit!(state);
                    for action in handler_fns::<S>(&state).on_exit {
                        action(&mut self.ctx);
                    }
                }
                for &state in target_path.iter() {
                    trace_on_entry!(state);
                    for action in handler_fns::<S>(&state).on_entry {
                        action(&mut self.ctx);
                    }
                }
            }
        }

        self.phase = MachinePhase::Operational(target);
    }
}
