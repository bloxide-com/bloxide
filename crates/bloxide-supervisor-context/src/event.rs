// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::{
    capability::BloxRuntime, event_tag::LifecycleEvent, lifecycle::LifecycleCommand,
    messaging::Envelope,
};

use crate::control::SupervisorControl;
#[cfg(feature = "dynamic")]
use crate::spawn::SpawnFactory;

/// The unified event type for supervisor state machines (static, no dynamic spawn).
///
/// Child lifecycle observations arrive here from the runtime's
/// run_supervised_actor loop, which converts DispatchOutcome into
/// ChildLifecycleEvent and delivers it to the supervisor's domain mailbox.
#[cfg(not(feature = "dynamic"))]
#[derive(Debug, Clone)]
pub enum SupervisorEvent<R: BloxRuntime> {
    Child(ChildLifecycleEvent),
    Control(SupervisorControl<R>),
    Lifecycle(LifecycleCommand),
}

/// The unified event type for supervisor state machines (dynamic, with spawn).
///
/// When the `dynamic` feature is enabled, this variant adds a `Spawn` mailbox
/// for receiving spawn requests typed by the application's `SpawnFactory`.
///
/// Note: The `From<Envelope<F::Request>>` impl that would be needed for the
/// tuple `Mailboxes` blanket impl is intentionally omitted — Rust's coherence
/// checker (E0119) rejects it because it cannot prove `F::Request` is never
/// `ChildLifecycleEvent`. Instead, the dynamic supervisor uses a custom
/// `SupervisorMailboxes` struct that maps stream items to event variants
/// directly in `poll_next()`, bypassing the `From` requirement entirely.
/// See `crates/bloxide-supervisor/src/dynamic_mailboxes.rs`.
#[cfg(feature = "dynamic")]
#[derive(Debug, Clone)]
pub enum SupervisorEvent<R: BloxRuntime, F: SpawnFactory<R>> {
    Child(ChildLifecycleEvent),
    Control(SupervisorControl<R>),
    Spawn(F::Request),
    Lifecycle(LifecycleCommand),
}

// ── EventTag impls ──────────────────────────────────────────────────────────

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> ::bloxide_core::event_tag::EventTag for SupervisorEvent<R> {
    fn event_tag(&self) -> u8 {
        match self {
            SupervisorEvent::Child(..) => 0u8,
            SupervisorEvent::Control(..) => 1u8,
            SupervisorEvent::Lifecycle(..) => ::bloxide_core::event_tag::LIFECYCLE_TAG,
        }
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> ::bloxide_core::event_tag::EventTag
    for SupervisorEvent<R, F>
{
    fn event_tag(&self) -> u8 {
        match self {
            SupervisorEvent::Child(..) => 0u8,
            SupervisorEvent::Control(..) => 1u8,
            SupervisorEvent::Spawn(..) => 2u8,
            SupervisorEvent::Lifecycle(..) => ::bloxide_core::event_tag::LIFECYCLE_TAG,
        }
    }
}

// ── Tag constants ───────────────────────────────────────────────────────────

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> SupervisorEvent<R> {
    pub const CHILD_TAG: u8 = 0u8;
    pub const CONTROL_TAG: u8 = 1u8;
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> SupervisorEvent<R, F> {
    pub const CHILD_TAG: u8 = 0u8;
    pub const CONTROL_TAG: u8 = 1u8;
    pub const SPAWN_TAG: u8 = 2u8;
}

// ── LifecycleEvent impls ─────────────────────────────────────────────────────

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> LifecycleEvent for SupervisorEvent<R> {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        match self {
            SupervisorEvent::Lifecycle(cmd) => Some(*cmd),
            _ => None,
        }
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> LifecycleEvent for SupervisorEvent<R, F> {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        match self {
            SupervisorEvent::Lifecycle(cmd) => Some(*cmd),
            _ => None,
        }
    }
}

// ── From impls ──────────────────────────────────────────────────────────────
//
// These `From<Envelope<M>>` impls are used by the tuple `Mailboxes` blanket
// impl for the **static** (non-dynamic) supervisor, which has only two
// streams (child + control). The dynamic supervisor uses a custom
// `SupervisorMailboxes` struct instead, so it does NOT need
// `From<Envelope<F::Request>>`.

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> From<Envelope<ChildLifecycleEvent>> for SupervisorEvent<R> {
    fn from(env: Envelope<ChildLifecycleEvent>) -> Self {
        SupervisorEvent::Child(env.1)
    }
}

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> From<Envelope<SupervisorControl<R>>> for SupervisorEvent<R> {
    fn from(env: Envelope<SupervisorControl<R>>) -> Self {
        SupervisorEvent::Control(env.1)
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> From<Envelope<ChildLifecycleEvent>>
    for SupervisorEvent<R, F>
{
    fn from(env: Envelope<ChildLifecycleEvent>) -> Self {
        SupervisorEvent::Child(env.1)
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> From<Envelope<SupervisorControl<R>>>
    for SupervisorEvent<R, F>
{
    fn from(env: Envelope<SupervisorControl<R>>) -> Self {
        SupervisorEvent::Control(env.1)
    }
}

// NOTE: No `From<Envelope<F::Request>>` impl — see the doc comment on
// `SupervisorEvent` above. The dynamic supervisor uses `SupervisorMailboxes`
// which maps stream items directly, bypassing the `From` requirement.

// ── SupervisorEventLike trait ───────────────────────────────────────────────
//
// A trait for pattern-matching on supervisor events. Implemented for both
// `SupervisorEvent<R>` (static) and `SupervisorEvent<R, F>` (dynamic) so that
// action functions can be generic over the event type.

/// Trait for extracting child and control events from a supervisor event.
pub trait SupervisorEventLike<R: BloxRuntime> {
    /// Returns the child lifecycle event if this is a `Child` variant.
    fn as_child_event(&self) -> Option<&ChildLifecycleEvent>;
    /// Returns the control event if this is a `Control` variant.
    fn as_control_event(&self) -> Option<&SupervisorControl<R>>;
}

#[cfg(not(feature = "dynamic"))]
impl<R: BloxRuntime> SupervisorEventLike<R> for SupervisorEvent<R> {
    fn as_child_event(&self) -> Option<&ChildLifecycleEvent> {
        match self {
            Self::Child(e) => Some(e),
            _ => None,
        }
    }
    fn as_control_event(&self) -> Option<&SupervisorControl<R>> {
        match self {
            Self::Control(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(feature = "dynamic")]
impl<R: BloxRuntime, F: SpawnFactory<R>> SupervisorEventLike<R> for SupervisorEvent<R, F> {
    fn as_child_event(&self) -> Option<&ChildLifecycleEvent> {
        match self {
            Self::Child(e) => Some(e),
            _ => None,
        }
    }
    fn as_control_event(&self) -> Option<&SupervisorControl<R>> {
        match self {
            Self::Control(e) => Some(e),
            _ => None,
        }
    }
}
