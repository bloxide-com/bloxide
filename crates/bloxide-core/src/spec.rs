// Copyright 2025 Bloxide, all rights reserved
use crate::capability::BloxRuntime;
use crate::event_tag::{EventTag, LifecycleEvent};
use crate::mailboxes::Mailboxes;
use crate::topology::StateTopology;
use crate::transition::StateRule;

/// Static handler table for a single state.
/// All function pointers are resolved at compile time.
pub struct StateFns<S: MachineSpec> {
    /// Called (in order) when entering this state during a transition. Infallible.
    pub on_entry: &'static [fn(&mut S::Ctx)],
    /// Called (in order) when exiting this state during a transition. Infallible.
    pub on_exit: &'static [fn(&mut S::Ctx)],
    /// Ordered transition rules. Evaluated in declaration order.
    pub transitions: &'static [StateRule<S>],
}

/// The core trait every state machine must implement.
///
/// # Engine-implicit Init and Root
///
/// Neither `Root` nor `Init` appear in the user's `State` enum:
///
/// - **Root** is engine-implicit. Top-level user states return `None` from
///   `StateTopology::parent()`. The engine handles lifecycle commands at
///   VirtualRoot level before state handlers. Domain events may bubble to
///   `root_transitions()` for global fallback handling.
///
/// - **Init** is engine-implicit. The engine calls `on_init_entry` only
///   when entering Init via Reset/Fail/Stop — **not** at initial construction.
///   It calls `on_init_exit` when leaving Init via Start command.
pub trait MachineSpec: Sized + 'static {
    type State: StateTopology;
    type Event: EventTag + LifecycleEvent + Send + 'static;
    type Ctx: 'static;
    type Mailboxes<R: BloxRuntime>: Mailboxes<Self::Event>;

    const HANDLER_TABLE: &'static [&'static StateFns<Self>];

    /// The first operational leaf state entered after Start command.
    fn initial_state() -> Self::State;

    /// Called when entering Init via Reset/Fail/Stop.
    fn on_init_entry(_ctx: &mut Self::Ctx) {}

    /// Called when leaving Init via Start command.
    fn on_init_exit(_ctx: &mut Self::Ctx) {}

    /// Returns true if state represents normal completion.
    fn is_terminal(_state: &Self::State) -> bool {
        false
    }

    /// Returns true if state represents a failure.
    /// Takes precedence over is_terminal if both true.
    fn is_error(_state: &Self::State) -> bool {
        false
    }

    /// Root-level transition rules for domain events.
    /// Evaluated when a domain event bubbles past all user states.
    /// Lifecycle commands are handled by the engine separately.
    fn root_transitions() -> &'static [StateRule<Self>] {
        &[]
    }
}
