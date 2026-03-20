// Copyright 2025 Bloxide, all rights reserved
use crate::messaging::ActorId;

/// Lifecycle commands sent to actors via their lifecycle mailbox.
/// Handled at VirtualRoot level, not in user state handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleCommand {
    /// Transition from Init to operational initial state.
    Start,
    /// Transition to Init, report Reset. Actor can be restarted.
    Reset,
    /// Transition to Init, report Stopped. Actor stays in Init.
    Stop,
    /// Health check - respond with Alive.
    Ping,
}

/// Events sent from a supervised child's run loop to the supervisor's
/// domain mailbox.
///
/// The runtime observes `DispatchOutcome` after every dispatch and
/// generates these events automatically. The supervisor's `MachineSpec`
/// handles them as normal domain events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildLifecycleEvent {
    /// Child exited Init and entered its initial state.
    Started { child_id: ActorId },
    /// Child entered a terminal state (`is_terminal()` returned true).
    Done { child_id: ActorId },
    /// Child entered an error state (`is_error()` returned true).
    /// Takes precedence over Done if both are true.
    Failed { child_id: ActorId },
    /// Child was reset to Init via Guard::Reset or LifecycleCommand::Reset.
    Reset { child_id: ActorId },
    /// Child was stopped permanently via LifecycleCommand::Stop.
    Stopped { child_id: ActorId },
    /// Child responded to a Ping — its run loop is healthy.
    Alive { child_id: ActorId },
}
