// Copyright 2025 Bloxide, all rights reserved
use crate::messaging::ActorId;

/// Lifecycle commands sent to actors via their lifecycle mailbox.
/// Handled at VirtualRoot level, not in user state handlers.
///
/// The four-level lifecycle model (`reset → stop → abort → kill`):
///
/// | Command  | Through dispatch? | Callbacks                    | End state              |
/// |----------|-------------------|------------------------------|------------------------|
/// | `Start`  | Yes               | `on_init_exit`               | `initial_state()`      |
/// | `Reset`  | Yes               | Full exit + entry chain       | `initial_state()`      |
/// | `Stop`   | Yes               | Full exit + `on_init_entry`  | `Init` (suspended)     |
/// | `Abort`  | No (mailbox)      | None                         | Task ends (cooperative) |
/// | `Ping`   | Yes               | None                         | Unchanged               |
///
/// `Kill` is not a `LifecycleCommand` — it is a runtime capability
/// (`KillCapability::kill(handle)`) that destroys the task externally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleCommand {
    /// Transition from Init to operational initial state.
    /// Fires `on_init_exit` (resource acquisition).
    Start,
    /// Reset directly to `initial_state()` — immediately operational.
    /// Fires the full exit chain for the current state, then the entry chain
    /// for `initial_state()`. Does NOT visit Init, does NOT fire
    /// `on_init_entry` or `on_init_exit`. The actor is immediately operational.
    Reset,
    /// Transition to Init, report Stopped. Actor is suspended.
    /// Fires the full exit chain, then `on_init_entry` (resource cleanup).
    /// Actor stays in Init until a `Start` command arrives.
    Stop,
    /// Health check - respond Alive.
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
    /// Also sent when a child self-resets via `Guard::Reset` or
    /// `LifecycleCommand::Reset` — both go directly to `initial_state()`.
    Started { child_id: ActorId },
    /// Child entered a terminal state (`is_terminal()` returned true).
    Done { child_id: ActorId },
    /// Child entered an error state (`is_error()` returned true) or
    /// returned `Guard::Fail`. Takes precedence over Done if both are true.
    Failed { child_id: ActorId },
    /// Child was stopped via `LifecycleCommand::Stop`.
    /// The exit chain and `on_init_entry` fired. The child is in Init,
    /// suspended, and can be restarted with `Start`.
    Stopped { child_id: ActorId },
    /// Child was aborted via `AbortCommand` on the abort mailbox.
    /// No callbacks fired — the child's task self-terminated cooperatively.
    /// The task has ended; restarting requires respawning the task.
    Aborted { child_id: ActorId },
    /// Child was killed via `KillCapability::kill(handle)` — the external ripcord.
    /// No callbacks fired — the child's task was destroyed externally and immediately.
    /// Permanently dead — cannot be restarted without respawning.
    Killed { child_id: ActorId },
    /// Child responded to a Ping — its run loop is healthy.
    Alive { child_id: ActorId },
}
