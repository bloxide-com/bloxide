// Copyright 2025 Bloxide, all rights reserved
// ── Imports ──────────────────────────────────────────────────────────────────

use crate::event_tag::{EventTag, LifecycleEvent, WILDCARD_TAG};
use crate::lifecycle::LifecycleCommand;
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

// ── MachineState ─────────────────────────────────────────────────────────────

/// Represents the current state of a machine, including the implicit Init.
///
/// Init is implicit (not part of the user's state enum) and tracked separately.
/// Users may have their own domain state also named "Init" with no conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineState<S> {
    /// Implicit Init state - machine is in lifecycle wait state.
    Init,
    /// One of the user's declared operational states.
    State(S),
}

impl<S> MachineState<S> {
    /// Returns true if the machine is in implicit Init.
    pub fn is_init(&self) -> bool {
        matches!(self, MachineState::Init)
    }

    /// Returns the operational state if present.
    pub fn as_state(&self) -> Option<&S> {
        match self {
            MachineState::Init => None,
            MachineState::State(s) => Some(s),
        }
    }
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
/// Returned by `dispatch()` so the runtime can observe lifecycle transitions
/// without coupling to actor event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome<State> {
    /// No rule matched anywhere (event bubbled to VirtualRoot with no match).
    NoRuleMatched,
    /// Rule matched but guard returned Stay.
    HandledNoTransition,
    /// Transition occurred to a user state.
    Transition(MachineState<State>),
    /// Left Init via Start command.
    Started(MachineState<State>),
    /// Transitioned to terminal state.
    Done(MachineState<State>),
    /// Actor reset to Init via Guard::Reset.
    Reset,
    /// Actor failed to Init via Guard::Fail or entered error state.
    Failed,
    /// Actor stopped to Init via LifecycleCommand::Stop.
    Stopped,
    /// Actor responded to Ping.
    Alive,
}

// ── StateMachine ─────────────────────────────────────────────────────────────

/// A hierarchical state machine.
///
/// Manages both the engine-implicit Init phase and the user-declared operational
/// state hierarchy. The machine starts in Init **silently** (no callbacks fire
/// on initial construction). Lifecycle commands (Start/Reset/Stop/Ping) flow
/// through `dispatch()` and are handled at VirtualRoot level.
///
/// `on_init_entry` fires only when the machine is **reset** (via
/// `Guard::Reset` or `Guard::Fail`), not on first construction.
pub struct StateMachine<S: MachineSpec> {
    /// Current state - either implicit Init or a user state.
    current: MachineState<S::State>,
    ctx: S::Ctx,
}

impl<S: MachineSpec> StateMachine<S> {
    /// Construct a new machine in implicit Init state.
    ///
    /// Init is entered SILENTLY - no callbacks fire. Construction is just
    /// setting the initial state. `on_init_entry` only fires when entering
    /// Init due to Reset/Fail/Stop.
    pub fn new(ctx: S::Ctx) -> Self {
        debug_assert!(
            S::HANDLER_TABLE.len() == S::State::STATE_COUNT,
            "HANDLER_TABLE len {} must equal State::STATE_COUNT {}",
            S::HANDLER_TABLE.len(),
            S::State::STATE_COUNT
        );
        debug_assert!(
            S::initial_state().is_leaf(),
            "initial_state() must return a leaf state"
        );
        trace_init_entry!();
        Self {
            current: MachineState::Init,
            ctx,
        }
    }

    /// Current state of the machine.
    pub fn current_state(&self) -> MachineState<S::State> {
        self.current
    }

    /// Mutable reference to context.
    pub fn ctx_mut(&mut self) -> &mut S::Ctx {
        &mut self.ctx
    }

    /// Shared reference to context.
    pub fn ctx(&self) -> &S::Ctx {
        &self.ctx
    }

    /// Dispatch an event through the state machine.
    ///
    /// Lifecycle commands (Start/Reset/Stop/Ping) are handled at VirtualRoot.
    /// Domain events flow through state handler tables, bubbling to root.
    pub fn dispatch(&mut self, event: S::Event) -> DispatchOutcome<S::State> {
        // Check for lifecycle commands first (VirtualRoot handling)
        if let Some(cmd) = event.as_lifecycle_command() {
            return self.handle_lifecycle(cmd);
        }

        // Domain event flow depends on current state
        match self.current {
            MachineState::Init => {
                // Init's auto-generated transitions catch all domain events
                // No callbacks, no state change - just stay in Init
                trace_init_drop_event!(&event);
                DispatchOutcome::HandledNoTransition
            }
            MachineState::State(current) => {
                trace_on_event_received!(current, &event);
                self.process_operational_event(event)
            }
        }
    }

    /// Handle lifecycle commands at VirtualRoot level.
    pub fn handle_lifecycle(&mut self, cmd: LifecycleCommand) -> DispatchOutcome<S::State> {
        match cmd {
            LifecycleCommand::Start => {
                match self.current {
                    MachineState::Init => {
                        // Transition from Init to user's initial state
                        let target = S::initial_state();
                        self.transition_to_state(target);
                        DispatchOutcome::Started(MachineState::State(target))
                    }
                    MachineState::State(_) => {
                        // Already operational - no-op
                        DispatchOutcome::HandledNoTransition
                    }
                }
            }
            LifecycleCommand::Reset => {
                // Transition to Init, report Reset
                self.transition_to_init();
                DispatchOutcome::Reset
            }
            LifecycleCommand::Stop => {
                // Transition to Init, report Stopped
                self.transition_to_init();
                DispatchOutcome::Stopped
            }
            LifecycleCommand::Ping => {
                // Respond Alive (runtime will send notification)
                DispatchOutcome::Alive
            }
        }
    }

    /// Process event while in operational state.
    fn process_operational_event(&mut self, event: S::Event) -> DispatchOutcome<S::State> {
        let current = match self.current {
            MachineState::State(s) => s,
            MachineState::Init => unreachable!("process_operational_event called while in Init"),
        };
        let event_tag = event.event_tag();
        let current_path = current.path();

        // Walk from leaf to root, evaluating state handlers
        for &ancestor in current_path.iter().rev() {
            let fns = handler_fns::<S>(&ancestor);

            if let Some(guard) =
                eval_rules::<S, Guard<S>>(fns.transitions, &mut self.ctx, &event, event_tag)
            {
                return self.apply_guard(guard);
            }
        }

        // Bubbled to VirtualRoot - check root transitions for domain events
        if let Some(guard) =
            eval_rules::<S, Guard<S>>(S::root_transitions(), &mut self.ctx, &event, event_tag)
        {
            return self.apply_guard(guard);
        }

        // No rule matched anywhere
        DispatchOutcome::NoRuleMatched
    }

    /// Apply a Guard outcome.
    fn apply_guard(&mut self, guard: Guard<S>) -> DispatchOutcome<S::State> {
        match guard {
            Guard::Transition(leaf) => {
                let target = leaf.into_inner();
                self.transition_to_state(target);

                // Check for terminal/error states
                if S::is_error(&target) {
                    DispatchOutcome::Failed
                } else if S::is_terminal(&target) {
                    DispatchOutcome::Done(MachineState::State(target))
                } else {
                    DispatchOutcome::Transition(MachineState::State(target))
                }
            }
            Guard::Stay => DispatchOutcome::HandledNoTransition,
            Guard::Reset => {
                self.transition_to_init();
                DispatchOutcome::Reset
            }
            Guard::Fail => {
                self.transition_to_init();
                DispatchOutcome::Failed
            }
        }
    }

    /// Transition to a user state (with LCA exit/entry callbacks).
    fn transition_to_state(&mut self, target: S::State) {
        match self.current {
            MachineState::Init => {
                // Exiting Init: fire on_init_exit, then enter target states
                trace_init_exit!();
                S::on_init_exit(&mut self.ctx);
                let path = target.path();
                for &state in path.iter() {
                    trace_on_entry!(state);
                    for action in handler_fns::<S>(&state).on_entry {
                        action(&mut self.ctx);
                    }
                }
                trace_on_transition!("Init", target, None::<&S::State>);
                self.current = MachineState::State(target);
            }
            MachineState::State(source) => {
                // Normal state-to-state transition with LCA
                self.change_state(source, target);
            }
        }
    }

    /// Transition to implicit Init (with LCA exit callbacks).
    fn transition_to_init(&mut self) {
        match self.current {
            MachineState::Init => {
                // Already in Init, nothing to do
            }
            MachineState::State(source) => {
                // Exit all states leaf-to-root
                let source_path = source.path();
                for &state in source_path.iter().rev() {
                    trace_on_exit!(state);
                    for action in handler_fns::<S>(&state).on_exit {
                        action(&mut self.ctx);
                    }
                }
                // Enter Init: fire on_init_entry
                trace_init_entry!();
                S::on_init_entry(&mut self.ctx);
                trace_on_transition!(source, "Init", None::<&S::State>);
                self.current = MachineState::Init;
            }
        }
    }

    /// State-to-state transition with LCA computation.
    fn change_state(&mut self, source: S::State, target: S::State) {
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

        trace_on_transition!(source, target, lca.map(|i| &target_path[i]));
        self.current = MachineState::State(target);
    }
}
