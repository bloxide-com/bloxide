// Copyright 2025 Bloxide, all rights reserved
//! Output types from spawn operations.

use core::fmt;

use bloxide_core::{
    capability::BloxRuntime,
    lifecycle::LifecycleCommand,
    messaging::{ActorId, ActorRef},
};

/// Supervision policy for a dynamically spawned child.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SpawnPolicy {
    /// Restart the child up to `max` times.
    Restart { max: usize },
    /// Stop the child permanently on failure.
    Stop,
    /// Kill the child immediately on failure.
    Kill,
}

/// Output from a successful spawn operation.
///
/// Returned by `SpawnFactoryFor::spawn()` to give the spawn service
/// the information needed to register the child with the supervisor.
pub struct SpawnOutput<R: BloxRuntime> {
    /// The allocated actor ID for the new child.
    pub child_id: ActorId,

    /// Channel for sending lifecycle commands (Start, Stop, Reset).
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,

    /// Optional supervision policy for this child.
    pub policy: Option<SpawnPolicy>,
}

impl<R: BloxRuntime> Clone for SpawnOutput<R> {
    fn clone(&self) -> Self {
        Self {
            child_id: self.child_id,
            lifecycle_ref: self.lifecycle_ref.clone(),
            policy: self.policy,
        }
    }
}

impl<R: BloxRuntime> fmt::Debug for SpawnOutput<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpawnOutput")
            .field("child_id", &self.child_id)
            .field("policy", &self.policy)
            .finish()
    }
}
