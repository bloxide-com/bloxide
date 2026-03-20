// Copyright 2025 Bloxide, all rights reserved
use core::fmt;

use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
};

use bloxide_core::lifecycle::LifecycleCommand;

use crate::registry::ChildPolicy;

/// Register a new child at runtime with the supervisor.
///
/// This enables dynamic supervision on runtimes that can spawn actors
/// dynamically (for example Tokio/TestRuntime) while keeping Embassy static
/// wiring unchanged.
pub struct RegisterChild<R: BloxRuntime> {
    pub id: ActorId,
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    pub policy: ChildPolicy,
}

impl<R: BloxRuntime> Clone for RegisterChild<R> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            lifecycle_ref: self.lifecycle_ref.clone(),
            policy: self.policy,
        }
    }
}

impl<R: BloxRuntime> fmt::Debug for RegisterChild<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterChild")
            .field("id", &self.id)
            .field("policy", &self.policy)
            .finish()
    }
}

/// Supervisor control-plane events delivered through a dedicated mailbox.
#[derive(Debug, Clone)]
pub enum SupervisorControl<R: BloxRuntime> {
    RegisterChild(RegisterChild<R>),
    /// Trigger one health-check round.
    HealthCheckTick,
}
