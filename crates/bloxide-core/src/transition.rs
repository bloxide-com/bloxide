// Copyright 2025 Bloxide, all rights reserved
use crate::spec::MachineSpec;
use crate::topology::LeafState;

// ── ActionResult ──────────────────────────────────────────────────────────────

/// The outcome of a single action function in a transition rule's action slice.
///
/// Action functions return `ActionResult` so the engine can collect all
/// outcomes before the guard makes its transition decision. Use
/// `ActionResult::from(result)` to convert any `Result<(), E>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionResult {
    Ok,
    Err,
}

/// Converts a `Result<(), E>` into an `ActionResult`.
///
/// **Note**: Error details are discarded. The guard receives only the
/// `any_failed()` boolean via `ActionResults`. If you need to preserve
/// error information for the guard, store it in the context before returning.
impl<E> From<Result<(), E>> for ActionResult {
    fn from(r: Result<(), E>) -> Self {
        if r.is_ok() {
            ActionResult::Ok
        } else {
            ActionResult::Err
        }
    }
}

/// Converts `()` into `ActionResult::Ok`.
///
/// Lets transition action functions return nothing: the system codegen wraps
/// every concrete transition call in `ActionResult::from(...)`, so action
/// functions may return `ActionResult`, `Result<(), E>`, or `()` — the use
/// site in the topology (transition vs entry/exit slot) determines the
/// closure shape, and the return type is inferred by rustc.
impl From<()> for ActionResult {
    fn from(_: ()) -> Self {
        ActionResult::Ok
    }
}

// ── ActionResults ─────────────────────────────────────────────────────────────

/// The collected outcomes of all action functions for one rule firing.
///
/// Created fresh by the engine for each event dispatch — never stored in `Ctx`.
/// Guards inspect `ActionResults` to decide the next state, enabling error
/// handling without polluting the actor context.
///
/// ```
/// use bloxide_core::transition::{ActionResult, ActionResults};
/// # #[derive(Debug, PartialEq)]
/// # enum MyState {
/// #     Error,
/// #     Done,
/// #     Running,
/// # }
/// # struct Ctx {
/// #     count: u32,
/// # }
/// # const MAX: u32 = 10;
///
/// // The engine collects every action outcome into `ActionResults` …
/// let results: ActionResults = [ActionResult::Ok, ActionResult::Err].into_iter().collect();
/// assert!(results.any_failed());
/// assert_eq!(results.failure_count(), 1);
///
/// // … then the guard picks the next state from `&Ctx` and `&ActionResults`:
/// # let ctx = Ctx { count: 3 };
/// let next = match () {
///     _ if results.any_failed() => MyState::Error,
///     _ if ctx.count >= MAX => MyState::Done,
///     _ => MyState::Running,
/// };
/// assert_eq!(next, MyState::Error);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ActionResults {
    failed: bool,
    failure_count: usize,
}

impl ActionResults {
    /// Create an empty (all-ok) results collector.
    pub const fn new() -> Self {
        Self {
            failed: false,
            failure_count: 0,
        }
    }

    /// Returns `true` if every action in the slice returned `ActionResult::Ok`.
    pub fn all_ok(&self) -> bool {
        !self.failed
    }

    /// Returns `true` if at least one action returned `ActionResult::Err`.
    pub fn any_failed(&self) -> bool {
        self.failed
    }

    /// The number of actions that returned `ActionResult::Err`.
    pub fn failure_count(&self) -> usize {
        self.failure_count
    }
}

impl Default for ActionResults {
    fn default() -> Self {
        Self::new()
    }
}

impl core::iter::FromIterator<ActionResult> for ActionResults {
    fn from_iter<I: IntoIterator<Item = ActionResult>>(iter: I) -> Self {
        let mut r = ActionResults::new();
        for item in iter {
            if matches!(item, ActionResult::Err) {
                r.failed = true;
                r.failure_count += 1;
            }
        }
        r
    }
}

// ── Unified rule struct ───────────────────────────────────────────────────────

/// Internal representation of a transition rule.
///
/// **Users should not name this type directly.** Use the `StateRule<S>` type alias instead,
/// which adds the `Decision<S>` type parameter.
///
/// This struct is public because `StateRule<S>` is a type alias that expands
/// to `TransitionRule<S, Decision<S>>`; Rust requires the aliased type to be at
/// least as visible as the alias. Users should use `StateRule<S>` in their
/// code.
///
/// # Enforcement
///
/// - Each function in `actions` receives `&mut Ctx` — side effects are allowed.
///   Each returns an [`ActionResult`] so send failures are visible to the guard.
/// - The engine iterates `actions`, collecting results into [`ActionResults`].
/// - `guard` receives `&Ctx` (immutable) and `&ActionResults` — the borrow
///   checker prevents context mutation, ensuring the guard is a pure decision.
/// - The engine always runs all actions before calling the guard.
pub struct TransitionRule<S: MachineSpec, G> {
    /// Fast discriminant tag for the event variant this rule handles.
    ///
    /// The engine pre-checks `event.event_tag() == event_tag` before calling
    /// `matches`, saving a function-pointer indirection for non-matching
    /// variants. [`WILDCARD_TAG`] (255) is the sentinel — rules with this tag
    /// always proceed to `matches` regardless of the event tag.
    ///
    /// Set automatically by `bloxide-codegen` when emitting `StateRule`
    /// struct literals from `[[topology.transitions]]` entries (both
    /// state-scope and root-scope). Manually-constructed rules should use
    /// the `*_TAG` constant from
    /// the event enum (e.g. `PingEvent::MSG_TAG`), or [`WILDCARD_TAG`] if the
    /// rule matches multiple variants.
    ///
    /// [`WILDCARD_TAG`]: crate::event_tag::WILDCARD_TAG
    pub event_tag: u8,

    /// Returns `true` if this rule applies to the given event.
    /// Called after `event_tag` pre-filter passes; if it returns `false`, the
    /// rule is skipped entirely.
    pub matches: fn(&S::Event) -> bool,

    /// Ordered slice of action functions. Each receives `&mut Ctx` and `&Event`
    /// and returns an [`ActionResult`]. The engine iterates the slice in order,
    /// collecting all results into [`ActionResults`] before calling `guard`.
    /// Use `&[]` for rules with no side effects.
    pub actions: &'static [ActionFn<S>],

    /// Pure transition decision. Takes `&Ctx` (read-only) and the collected
    /// `&ActionResults` to enforce that all mutations already happened in
    /// `actions`. Returns `G` to determine the next engine action.
    pub guard: fn(&S::Ctx, &ActionResults, &S::Event) -> G,
}

/// A single action function: receives mutable context and the triggering event,
/// returns an [`ActionResult`] indicating success or failure.
pub type ActionFn<S> = fn(&mut <S as MachineSpec>::Ctx, &<S as MachineSpec>::Event) -> ActionResult;

#[cfg(test)]
mod tests {
    use super::ActionResult;

    #[test]
    fn action_result_from_conversions() {
        // Transition action fns may return ActionResult, Result<(), E>, or ();
        // the codegen wrapper normalizes all three via ActionResult::from.
        assert_eq!(ActionResult::from(ActionResult::Err), ActionResult::Err);
        assert_eq!(
            ActionResult::from(Ok::<(), &'static str>(())),
            ActionResult::Ok
        );
        assert_eq!(
            ActionResult::from(Err::<(), &'static str>("boom")),
            ActionResult::Err
        );
        assert_eq!(ActionResult::from(()), ActionResult::Ok);
    }
}

/// State-level transition rule. The guard closure returns a [`Decision<S>`].
pub type StateRule<S> = TransitionRule<S, Decision<S>>;

// ── Decision outcomes ───────────────────────────────────────────────────────────

/// The outcome of a guard evaluation (state-level or root-level).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision<S: MachineSpec> {
    /// Perform a transition to `target`. When `target == current_state` this
    /// is a self-transition: fires `on_exit` then `on_entry`.
    ///
    /// Takes a `LeafState<S::State>` so composite states cannot be transition
    /// targets. `bloxide-codegen` wraps targets in `LeafState::new` automatically
    /// when emitting `StateRule` struct literals from `[[topology.transitions]]`
    /// entries — user-facing syntax is unchanged.
    Transition(LeafState<S::State>),
    /// Stay in the current state. No `on_exit` or `on_entry` is called.
    Stay,
    /// Self-reset: go directly to `initial_state()`, skipping Init entirely.
    /// Uses LCA-based `change_state` (ancestors at/above the LCA do not fire
    /// `on_exit`), then the entry chain for `initial_state()`. Does NOT call `on_init_entry` or `on_init_exit`.
    /// The actor is immediately operational.
    Reset,
    /// Self-suspend: go to Init (fire exit chain + `on_init_entry`).
    /// Reports `DispatchOutcome::Stopped` to the supervisor.
    /// Actor is suspended in Init and can be restarted with `Start` or `Reset`.
    Stop,
    /// Self-terminate cleanly: fire the full exit chain + `on_init_entry`
    /// (same cleanup ritual as `Stop`), then the task ENDS instead of
    /// suspending. Reports `DispatchOutcome::Done` to the supervisor, which
    /// deregisters the child — no restart policy triggered.
    /// Use for normal completion; use `Stop` for suspend/resume.
    Done,
    /// Error propagation. Go to user-defined `error_state()` if one exists,
    /// otherwise go to Init (firing exit chain + `on_init_entry`).
    /// Reports `DispatchOutcome::Failed` to the supervisor.
    Fail,
}
