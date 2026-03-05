// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::messaging::Envelope;
use bloxide_macros::EventTag;

use crate::lifecycle::ChildLifecycleEvent;

/// The unified event type for supervisor state machines.
///
/// Child lifecycle observations arrive here from the runtime's
/// run_supervised_actor loop, which converts DispatchOutcome into
/// ChildLifecycleEvent and delivers it to the supervisor's domain mailbox.
///
/// The supervisor's own lifecycle (Start/Terminate/Ping from its parent)
/// is handled directly by the runtime via StateMachine::start() and
/// StateMachine::reset() — no longer dispatched as domain events.
#[derive(Debug, EventTag)]
pub enum SupervisorEvent {
    Child(ChildLifecycleEvent),
}

impl From<Envelope<ChildLifecycleEvent>> for SupervisorEvent {
    fn from(env: Envelope<ChildLifecycleEvent>) -> Self {
        SupervisorEvent::Child(env.1)
    }
}
