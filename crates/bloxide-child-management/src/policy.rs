// Copyright 2025 Bloxide, all rights reserved
//! Supervision policies — what the managing blox does when a child fails
//! (`ChildPolicy`) and when group-level shutdown triggers (`GroupShutdown`).

/// Supervision policy for a child actor.
///
/// Determines what the managing blox does when the child fails (reports
/// `Stopped` or `Failed`, or misses `MAX_MISSES` consecutive health checks).
///
/// The five-level lifecycle model (`reset → stop → done → abort → kill`):
///
/// | Policy | Mechanism | Cooperative? | Callbacks? | Revivable? |
/// |--------|-----------|-------------|------------|------------|
/// | `Reset { .. }` | Send `Reset` | Yes | Exit + entry chain | Yes (immediately) |
/// | `Stop` | No command — child marked done for this epoch | — | — | No |
/// | `Abort` | Send `AbortCommand` on abort mailbox | Yes (cooperative) | None | Yes (respawn task) |
/// | `Kill` | `KillCapability::kill(handle)` | No (forced) | None | No (permanently dead) |
///
/// `Abort` and `Kill` require a dynamically spawned child (registered via
/// `ChildGroup::try_add_dynamic` with abort/kill handles); `try_add` rejects
/// them with `RegistrationError::PolicyRequiresHandles`. `Kill` additionally
/// requires a runtime with `KillCapability::CAN_KILL` (`NoKill` runtimes such
/// as Embassy reject it with `RegistrationError::KillUnavailable`) — killing
/// is a no-op there, and marking a live child `Killed` would corrupt the
/// group's bookkeeping.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChildPolicy {
    /// Send `Reset` to the child — goes directly to `initial_state()`.
    /// The actor is immediately operational. No need to send `Start` separately.
    /// The child revives and continues running.
    ///
    /// `max` caps **consecutive** restarts: the counter increments on each
    /// delivered `Reset` and resets to zero when the child reports `Started`
    /// and then answers a health `Ping` with `Alive` (a no-clock proxy for
    /// sustained uptime). Once the child has been reset `max` times in a row,
    /// the next failure gives up: the child is marked `Stopped` (terminal,
    /// task alive) and group shutdown is evaluated. Without watchdog ticks,
    /// `Alive` never arrives, so `max` degrades to a lifetime restart cap —
    /// the fail-safe direction.
    Reset { max: u32 },
    /// Leave the child as-is and mark it done for this epoch (counts toward
    /// group shutdown). No command is sent: the child already self-stopped
    /// (suspended in Init) or failed (parked in its error state).
    Stop,
    /// Send `AbortCommand` on the abort mailbox for cooperative self-termination.
    /// No callbacks fire. The child's task ends. Requires the child to have
    /// an abort capability mailbox.
    Abort,
    /// Immediately kill the child via `KillCapability::kill(handle)`.
    /// The task is destroyed externally — no callbacks, no cooperation.
    /// Permanently dead. Requires the child to have a kill capability
    /// (kill handle from `SpawnCap`) and a runtime with `CAN_KILL`.
    Kill,
}

/// When to trigger group-level shutdown.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GroupShutdown {
    WhenAnyDone,
    WhenAllDone,
}
