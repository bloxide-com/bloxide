// Copyright 2025 Bloxide, all rights reserved
//! Child management types — policies, kill commands, and group shutdown.
//!
//! These types live in `bloxide-core` because they are NOT supervisor-specific.
//! Any blox that manages children — our standard supervisor, a user's custom
//! job dispatcher, a load balancer — needs the same policy and kill types.
//! See spec/architecture/22-spawn-architecture-v2.md §4.4, §4.5.

use crate::messaging::ActorId;

/// Command enum for the kill capability mailbox.
///
/// Sent by the managing blox (supervisor or custom) when `ChildPolicy::Kill` fires.
/// The child's task receives this on its kill mailbox and aborts itself.
///
/// This is the first instance of the capability-as-mailbox pattern.
/// Future capabilities (suspend, resume, inspect) will follow the same
/// pattern: a command enum sent on a per-child mailbox.
#[derive(Debug, Clone)]
pub enum KillCommand {
    /// Kill the child immediately. No callbacks, no graceful shutdown.
    /// The child's task aborts on receipt.
    Kill { child_id: ActorId },
}

/// Supervision policy for a child actor.
///
/// Determines what the managing blox does when the child fails (reports
/// `Done` or `Failed`).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChildPolicy {
    /// Restart the child up to `max` times. `max` is the number of restart
    /// *attempts* allowed: after the `max`-th restart the next failure triggers
    /// group shutdown. `Restart { max: 0 }` means no restarts — equivalent to `Stop`.
    Restart { max: usize },
    /// Send Stop command for clean shutdown (callbacks run).
    Stop,
    /// Immediately kill the child without callbacks.
    /// Requires the child to have a kill capability mailbox.
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
