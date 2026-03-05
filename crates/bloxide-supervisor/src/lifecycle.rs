use bloxide_core::messaging::ActorId;

/// Commands sent from a supervisor to a supervised child actor.
///
/// These are delivered via `ActorRef<LifecycleCommand, R>` — the child's
/// lifecycle channel. The runtime's `SupervisedRunLoop` implementation
/// receives these and controls the child's `StateMachine`.
#[derive(Debug)]
pub enum LifecycleCommand {
    /// Transition from Init to the initial operational state.
    Start,
    /// Exit all operational states back to Init. The task stays alive
    /// and can receive a subsequent `Start` to resume.
    Terminate,
    /// Exit all operational states, re-enter Init, then permanently
    /// exit the task. The child cannot be restarted.
    Stop,
    /// Health check probe. The run loop responds with
    /// `ChildLifecycleEvent::Alive`.
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
    /// Takes precedence over `Done` if both are true.
    Failed { child_id: ActorId },
    /// Child was reset to Init via Terminate and is waiting for Start.
    Reset { child_id: ActorId },
    /// Child was stopped permanently — its task has exited.
    Stopped { child_id: ActorId },
    /// Child responded to a Ping — its run loop is healthy.
    Alive { child_id: ActorId },
}
