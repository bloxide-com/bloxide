// Copyright 2025 Bloxide, all rights reserved
//! Child management types — policies, abort commands, and group shutdown.
//!
//! These types live in `bloxide-core` because they are NOT supervisor-specific.
//! Any blox that manages children — our standard supervisor, a custom
//! job dispatcher, a load balancer — needs the same policy and abort types.

use crate::messaging::ActorId;

/// Command enum for the abort capability mailbox.
///
/// Sent by the managing blox (supervisor or custom) when `ChildPolicy::Abort` fires.
/// The child's task receives this on its abort mailbox and self-terminates
/// cooperatively (breaks out of the run loop, no callbacks fire).
///
/// This is the cooperative self-termination path — distinct from `KillCapability`
/// which is the external ripcord that destroys the task without cooperation.
#[derive(Debug, Clone)]
pub enum AbortCommand {
    /// Abort the child cooperatively. No callbacks, no graceful shutdown.
    /// The child's task self-terminates on receipt.
    Abort { child_id: ActorId },
}

/// Supervision policy for a child actor.
///
/// Determines what the managing blox does when the child fails (reports
/// `Done` or `Failed`).
///
/// The four-level lifecycle model (`reset → stop → abort → kill`):
///
/// | Policy | Mechanism | Cooperative? | Callbacks? | Restartable? |
/// |--------|-----------|-------------|------------|--------------|
/// | `Restart` | Send `Reset` | Yes | Exit + entry chain | Yes (immediately) |
/// | `Stop` | Send `Stop` | Yes | Exit + `on_init_entry` | Yes (via `Start`) |
/// | `Abort` | Send `AbortCommand` on abort mailbox | Yes (cooperative) | None | Yes (respawn task) |
/// | `Kill` | `KillCapability::kill(handle)` | No (forced) | None | No (permanently dead) |
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChildPolicy {
    /// Restart the child up to `max` times by sending `Reset`.
    /// `Reset` goes directly to `initial_state()` — the actor is immediately
    /// operational. `max` is the number of restart attempts allowed: after
    /// the `max`-th restart the next failure triggers group shutdown.
    /// `Restart { max: 0 }` means no restarts — equivalent to `Stop`.
    Restart { max: usize },
    /// Send `Stop` command for clean shutdown (exit chain + `on_init_entry` fire).
    /// Actor goes to Init, suspended, can be restarted with `Start`.
    Stop,
    /// Send `AbortCommand` on the abort mailbox for cooperative self-termination.
    /// No callbacks fire. The child's task ends. Requires the child to have
    /// an abort capability mailbox.
    Abort,
    /// Immediately kill the child via `KillCapability::kill(handle)`.
    /// The task is destroyed externally — no callbacks, no cooperation.
    /// Permanently dead. Requires the child to have a kill capability
    /// (abort handle from `SpawnCap`).
    Kill,
}

/// Group-level restart strategy determining which children are restarted
/// when a child fails. Inspired by Erlang/OTP supervisor strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RestartStrategy {
    /// Restart only the failed child (default).
    #[default]
    OneForOne,
    /// Restart all children when any child fails.
    OneForAll,
    /// Restart the failed child and all children declared after it.
    RestForOne,
}

/// When to trigger group-level shutdown.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GroupShutdown {
    WhenAnyDone,
    WhenAllDone,
}
