// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::{
    capability::BloxRuntime, event_tag::LifecycleEvent, lifecycle::LifecycleCommand,
    messaging::Envelope,
};

use crate::control::SupervisorControl;

/// The unified event type for supervisor state machines.
///
/// Child lifecycle observations arrive here from the runtime's
/// run_supervised_actor loop, which converts DispatchOutcome into
/// ChildLifecycleEvent and delivers it to the supervisor's domain mailbox.
///
/// Lifecycle variants for supervised supervisor (if parented):
/// - Lifecycle(Start) transitions from Init to Running
/// - Lifecycle(Reset) transitions to Init
/// - Lifecycle(Stop) transitions to Init and exits
/// - Lifecycle(Ping) responds with Alive
#[derive(Debug, Clone)]
pub enum SupervisorEvent<R: BloxRuntime> {
    Child(ChildLifecycleEvent),
    Control(SupervisorControl<R>),
    Lifecycle(LifecycleCommand),
}

impl<R: BloxRuntime> ::bloxide_core::event_tag::EventTag for SupervisorEvent<R> {
    fn event_tag(&self) -> u8 {
        match self {
            SupervisorEvent::Child(..) => 0u8,
            SupervisorEvent::Control(..) => 1u8,
            SupervisorEvent::Lifecycle(..) => ::bloxide_core::event_tag::LIFECYCLE_TAG,
        }
    }
}

impl<R: BloxRuntime> SupervisorEvent<R> {
    pub const CHILD_TAG: u8 = 0u8;
    pub const CONTROL_TAG: u8 = 1u8;
}

impl<R: BloxRuntime> LifecycleEvent for SupervisorEvent<R> {
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
        match self {
            SupervisorEvent::Lifecycle(cmd) => Some(*cmd),
            _ => None,
        }
    }
}

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
