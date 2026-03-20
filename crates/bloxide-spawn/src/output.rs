// Copyright 2025 Bloxide, all rights reserved
//! Output types from spawn operations.

use core::fmt;

use bloxide_core::{
    lifecycle::LifecycleCommand,
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
};
use bloxide_supervisor::registry::ChildPolicy;

/// Output from a successful spawn operation.
///
/// Returned by `SpawnFactory::spawn()` to give the spawn service
/// the information needed to register the child with the supervisor.
///
/// # Fields
///
/// - `child_id`: The allocated actor ID for the new child.
/// - `lifecycle_ref`: Channel for sending lifecycle commands (Start, Stop, Reset).
///   The supervisor holds this to control the child's lifecycle.
/// - `policy`: Optional supervision policy for this child.
///   If `None`, the supervisor uses its default policy for new children.
///
/// # Notes
///
/// - The `reply_to` handling is the factory's responsibility.
///   Factory sends the typed reply (if provided) before returning.
/// - Domain refs are sent via the typed reply, not included here.
///   This keeps `SpawnOutput` focused on supervision concerns.
pub struct SpawnOutput<R: BloxRuntime> {
    /// The ID of the spawned child actor.
    pub child_id: ActorId,

    /// Channel for lifecycle commands (Start, Stop, Reset).
    /// The supervisor uses this to control the child.
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,

    /// Optional supervision policy for this child.
    /// If `None`, supervisor uses its default.
    pub policy: Option<ChildPolicy>,
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

impl<R: BloxRuntime> fmt::Display for SpawnOutput<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SpawnOutput {{ child_id: {}, policy: {:?} }}",
            self.child_id, self.policy
        )
    }
}
