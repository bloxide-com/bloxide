// Copyright 2025 Bloxide, all rights reserved
use core::fmt;

use bloxide_core::{
    capability::{BloxRuntime, KillCapability},
    child_management::{ChildPolicy, KillCommand},
    lifecycle::LifecycleCommand,
    messaging::{ActorId, ActorRef},
};

/// Register a static child (wired at startup). No kill capability.
/// Used by the wiring layer for Embassy and static Tokio children.
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

/// Register a dynamically spawned child. Has a kill capability mailbox.
/// Used by the spawn helper when SpawnCap is available.
///
/// The `kill_ref` and `task_handle` fields are for the kill capability —
/// see spec/architecture/22-spawn-architecture-v2.md §4.7.
///
/// This type does NOT implement `Clone` because `task_handle`
/// (`JoinHandle<()>` on Tokio) is not `Clone`. Messages are sent by value
/// via `try_send`, so `Clone` is not required by the messaging system.
pub struct RegisterDynamicChild<R: BloxRuntime> {
    pub id: ActorId,
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    /// Kill capability mailbox (send side). The supervisor sends `KillCommand`
    /// here; the child's task receives it and self-terminates.
    pub kill_ref: ActorRef<KillCommand, R>,
    /// Task handle for external abort (the ripcord). Stored by value.
    /// `()` for NoKill runtimes, `R::TaskHandle` for Kill runtimes.
    pub task_handle: <R::Kill as KillCapability<R>>::Handle,
    pub policy: ChildPolicy,
}

impl<R: BloxRuntime> fmt::Debug for RegisterDynamicChild<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterDynamicChild")
            .field("id", &self.id)
            .field("policy", &self.policy)
            .finish()
    }
}

/// Supervisor control-plane events delivered through a dedicated mailbox.
///
/// There is no `Spawn` variant — spawning is decoupled from the supervisor.
/// The spawn helper calls `spawn_child()` (in `bloxide-core`) which sends
/// `RegisterDynamicChild` on the control mailbox after the child is created.
///
/// Does NOT derive `Clone` because `RegisterDynamicChild` contains a
/// `task_handle` that is not `Clone` on Tokio.
pub enum SupervisorControl<R: BloxRuntime> {
    /// Register a static child (wired at startup, no kill capability).
    RegisterChild(RegisterChild<R>),
    /// Register a dynamically spawned child (has kill capability).
    RegisterDynamicChild(RegisterDynamicChild<R>),
    /// Trigger one health-check round.
    HealthCheckTick,
}

impl<R: BloxRuntime> fmt::Debug for SupervisorControl<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegisterChild(r) => f.debug_tuple("RegisterChild").field(r).finish(),
            Self::RegisterDynamicChild(r) => {
                f.debug_tuple("RegisterDynamicChild").field(r).finish()
            }
            Self::HealthCheckTick => write!(f, "HealthCheckTick"),
        }
    }
}

/// Marker type for the standard supervisor's `ChildRegistrar` implementation.
///
/// The spawn helper (`spawn_child`) is generic over `C: ChildRegistrar<R>`.
/// For the standard supervisor, `C = SupervisorRegistrar`. The wiring layer
/// injects this type when the supervisor is the managing blox.
pub struct SupervisorRegistrar;

impl<R: BloxRuntime> bloxide_core::spawn::ChildRegistrar<R> for SupervisorRegistrar {
    type RegisterMsg = SupervisorControl<R>;

    fn register(output: bloxide_core::spawn::SpawnOutput<R>) -> SupervisorControl<R> {
        SupervisorControl::RegisterDynamicChild(RegisterDynamicChild {
            id: output.child_id,
            lifecycle_ref: output.lifecycle_ref,
            kill_ref: output.kill_ref,
            task_handle: output.task_handle,
            policy: output.policy,
        })
    }
}
