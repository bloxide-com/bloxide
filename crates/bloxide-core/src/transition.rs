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

// ── ActionResults ─────────────────────────────────────────────────────────────

/// The collected outcomes of all action functions for one rule firing.
///
/// Created fresh by the engine for each event dispatch — never stored in `Ctx`.
/// Guards inspect `ActionResults` to decide the next state, enabling error
/// handling without polluting the actor context.
///
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// guard(ctx, results, event) {
///     results.any_failed() => MyState::Error,
///     ctx.count >= MAX     => MyState::Done,
///     _                    => MyState::Running,
/// }
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
/// which adds the `Guard<S>` type parameter.
///
/// This type is kept public for proc-macro generated code compatibility, but
/// the type alias is the preferred API surface.
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
    /// Set automatically by the `transitions!` and `root_transitions!` proc
    /// macros. Manually-constructed rules should use the `*_TAG` constant from
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

/// State-level transition rule. Guard returns [`Guard<S>`] (Transition or Stay).
pub type StateRule<S> = TransitionRule<S, Guard<S>>;

// ── Guard outcomes ───────────────────────────────────────────────────────────

/// The outcome of a guard evaluation (state-level or root-level).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Guard<S: MachineSpec> {
    /// Perform a transition to `target`. When `target == current_state` this
    /// is a self-transition: fires `on_exit` then `on_entry`.
    ///
    /// Takes a `LeafState<S::State>` so composite states cannot be transition
    /// targets. The `transitions!` proc macro wraps targets in `LeafState::new`
    /// automatically — user-facing syntax is unchanged.
    Transition(LeafState<S::State>),
    /// Stay in the current state. No `on_exit` or `on_entry` is called.
    Stay,
    /// Exit the entire operational state chain and re-enter engine-implicit Init.
    /// The engine calls `on_exit` for every state from the current leaf up to
    /// the topmost ancestor, then calls `MachineSpec::on_init_entry`.
    Reset,
    /// Exit to implicit Init, report Failed to supervisor.
    /// Used for error propagation that should trigger supervisor intervention.
    Fail,
}
