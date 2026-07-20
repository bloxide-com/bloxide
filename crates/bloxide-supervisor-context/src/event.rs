// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::{
    capability::BloxRuntime,
    event_tag::LifecycleEvent,
    lifecycle::LifecycleCommand,
    messaging::Envelope,
};

use crate::control::SupervisorControl;

/// The unified event type for supervisor state machines.
///
/// Child lifecycle observations arrive here from the runtime's
/// `run_supervised_actor` loop, which converts DispatchOutcome into
/// `ChildLifecycleEvent` and delivers it to the supervisor's domain mailbox.
///
/// There is a single `SupervisorEvent<R>` — no `F` parameter, no `Spawn`
/// variant. Spawning is decoupled from the supervisor: the requesting blox
/// calls `bloxide_core::spawn::spawn_child` directly, and the supervisor
/// receives `RegisterDynamicChild` via its control channel.
pub enum SupervisorEvent<R: BloxRuntime> {
    Child(ChildLifecycleEvent),
    Control(SupervisorControl<R>),
    Lifecycle(LifecycleCommand),
}

// ── EventTag impls ──────────────────────────────────────────────────────────

impl<R: BloxRuntime> ::bloxide_core::event_tag::EventTag for SupervisorEvent<R> {
    fn event_tag(&self) -> u8 {
        match self {
            SupervisorEvent::Child(..) => 0u8,
            SupervisorEvent::Control(..) => 1u8,
            SupervisorEvent::Lifecycle(..) => ::bloxide_core::event_tag::LIFECYCLE_TAG,
        }
    }
}

// ── Tag constants ───────────────────────────────────────────────────────────

impl<R: BloxRuntime> SupervisorEvent<R> {
    pub const CHILD_TAG: u8 = 0u8;
    pub const CONTROL_TAG: u8 = 1u8;
}

// ── LifecycleEvent impls ─────────────────────────────────────────────────────

impl<R: BloxRuntime> LifecycleEvent for SupervisorEvent<R> {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        match self {
            SupervisorEvent::Lifecycle(cmd) => Some(*cmd),
            _ => None,
        }
    }
}

// ── From impls ──────────────────────────────────────────────────────────────

impl<R: BloxRuntime> From<Envelope<ChildLifecycleEvent>> for SupervisorEvent<R> {
    fn from(env: Envelope<ChildLifecycleEvent>) -> Self {
        SupervisorEvent::Child(env.1)
    }
}

impl<R: BloxRuntime> From<Envelope<SupervisorControl<R>>> for SupervisorEvent<R> {
    fn from(env: Envelope<SupervisorControl<R>>) -> Self {
        SupervisorEvent::Control(env.1)
    }
}

// ── SupervisorEventLike trait ───────────────────────────────────────────────

/// Trait for extracting child and control events from a supervisor event.
pub trait SupervisorEventLike<R: BloxRuntime> {
    fn as_child_event(&self) -> Option<&ChildLifecycleEvent>;
    fn as_control_event(&self) -> Option<&SupervisorControl<R>>;
}

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
