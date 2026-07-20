// Copyright 2025 Bloxide, all rights reserved
use core::fmt;

use bloxide_core::{
    capability::{BloxRuntime, KillCapability},
    child_management::{AbortCommand, ChildPolicy},
    lifecycle::LifecycleCommand,
    messaging::{ActorId, ActorRef},
};

/// Register a static child (wired at startup). No abort capability.
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

/// Register a dynamically spawned child. Has an abort capability mailbox
/// and a kill handle (ripcord).
///
/// The `abort_ref` is for cooperative self-termination (the child polls its
/// abort mailbox and self-terminates on receipt of `AbortCommand`).
/// The `abort_handle` is the external ripcord (`KillCapability::kill(handle)`).
///
/// This type implements `Clone` because `abort_handle` is `Clone`
/// (it's `R::AbortHandle`, which requires `Clone` on the `SpawnCap` trait).
/// This allows the supervisor's action function to clone the `abort_handle`
/// from `&Event` (the HSM engine passes `&Event`, not `&mut Event`).
//
// NOTE: Manual `Clone` impl (not `#[derive(Clone)]`) because the derive
// macro generates `R: Clone` bounds that don't imply
// `<R::Kill as KillCapability<R>>::Handle: Clone`. The manual impl uses
// `R: BloxRuntime` which implies `R::Kill: KillCapability<R>` which implies
// `Handle: Clone`.
pub struct RegisterDynamicChild<R: BloxRuntime> {
    pub id: ActorId,
    pub lifecycle_ref: ActorRef<LifecycleCommand, R>,
    /// Abort capability mailbox (send side). The supervisor sends
    /// `AbortCommand` here; the child's task receives it and self-terminates
    /// cooperatively (no callbacks, no dispatch).
    pub abort_ref: ActorRef<AbortCommand, R>,
    /// Cloneable abort handle for external task kill (the ripcord).
    /// `()` for NoKill runtimes, `R::AbortHandle` for Kill runtimes.
    /// Must be `Clone` so the action function can extract it from `&Event`.
    pub abort_handle: <R::Kill as KillCapability<R>>::Handle,
    pub policy: ChildPolicy,
}

impl<R: BloxRuntime> Clone for RegisterDynamicChild<R> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            lifecycle_ref: self.lifecycle_ref.clone(),
            abort_ref: self.abort_ref.clone(),
            abort_handle: self.abort_handle.clone(),
            policy: self.policy,
        }
    }
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
/// Implements `Clone` because all variants are `Clone` (`RegisterDynamicChild`
/// uses `abort_handle` which is `Clone`). Manual impl (not `#[derive]`) to
/// avoid the derive macro generating `R: Clone` bounds.
pub enum SupervisorControl<R: BloxRuntime> {
    /// Register a static child (wired at startup, no abort capability).
    RegisterChild(RegisterChild<R>),
    /// Register a dynamically spawned child (has abort capability + kill handle).
    RegisterDynamicChild(RegisterDynamicChild<R>),
    /// Trigger one health-check round.
    HealthCheckTick,
}

impl<R: BloxRuntime> Clone for SupervisorControl<R> {
    fn clone(&self) -> Self {
        match self {
            Self::RegisterChild(r) => Self::RegisterChild(r.clone()),
            Self::RegisterDynamicChild(r) => Self::RegisterDynamicChild(r.clone()),
            Self::HealthCheckTick => Self::HealthCheckTick,
        }
    }
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
            abort_ref: output.abort_ref,
            abort_handle: output.abort_handle,
            policy: output.policy,
        })
    }
}
