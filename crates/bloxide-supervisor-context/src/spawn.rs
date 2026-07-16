// Copyright 2025 Bloxide, all rights reserved
//! Spawn factory and output types for dynamic actor spawning.

use core::fmt;

use bloxide_core::{
    capability::BloxRuntime,
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::{ActorId, ActorRef},
};

/// A spawn factory creates child actors.
///
/// Implementations are concrete structs (by value, no dyn).
/// The application provides the factory at wiring time.
/// The factory calls R::spawn() internally — the supervisor never does.
pub trait SpawnFactory<R: BloxRuntime> {
    /// Application-specific spawn request enum.
    /// Carries typed reply channels — no type erasure needed.
    type Request: Send + Clone + 'static;

    /// Spawn a child actor.
    ///
    /// Called by the supervisor's handle_spawn_request action.
    /// `notify` is the supervisor's child-event mailbox — the factory
    /// passes it to run_supervised_actor so the child can report
    /// lifecycle events (Done, Failed, Reset, etc.) back to the supervisor.
    fn spawn(&self, req: Self::Request, notify: ActorRef<ChildLifecycleEvent, R>)
        -> SpawnOutput<R>;
}

/// Accessor trait for the child notify channel.
pub trait HasChildNotify<R: BloxRuntime> {
    fn child_notify(&self) -> &ActorRef<ChildLifecycleEvent, R>;
}

/// Accessor trait for the spawn factory.
pub trait HasSpawnFactory<R: BloxRuntime> {
    type Factory: SpawnFactory<R>;
    fn spawn_factory(&self) -> &Self::Factory;
}

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

/// A no-op spawn factory for apps that don't use dynamic spawning.
/// Used when the 'dynamic' feature is enabled by feature unification
/// but the app doesn't actually need spawning.
pub struct NoSpawnFactory;

/// Placeholder request type for NoSpawnFactory.
#[derive(Clone)]
pub enum NoSpawnRequest {}

impl<R: BloxRuntime> SpawnFactory<R> for NoSpawnFactory {
    type Request = NoSpawnRequest;
    fn spawn(
        &self,
        _req: Self::Request,
        _notify: ActorRef<ChildLifecycleEvent, R>,
    ) -> SpawnOutput<R> {
        // This can never be called because NoSpawnRequest has no variants
        match _req {}
    }
}

/// Output from a successful spawn operation.
///
/// Returned by `SpawnFactory::spawn()` to give the supervisor
/// the information needed to register the child.
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
