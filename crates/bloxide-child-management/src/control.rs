// Copyright 2025 Bloxide, all rights reserved
//! Child-management control-plane messages.
//!
//! The managing blox (the standard supervisor, or a custom one) receives
//! registration and health-check messages on a dedicated control mailbox.
//! The standard supervisor blox is the reference consumer — these types are
//! owned by the platform, not by any blox (spec 18: Platform Feature Pattern).
use core::fmt;

use crate::ChildPolicy;
use bloxide_core::lifecycle::AbortCommand;
use bloxide_core::{
    capability::{BloxRuntime, KillCapability},
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
/// The `kill_handle` is the external ripcord (`KillCapability::kill(handle)`).
///
/// This type implements `Clone` because `kill_handle` is `Clone`
/// (it's `R::KillHandle`, which requires `Clone` on the `SpawnCap` trait).
/// This allows the managing blox's action function to clone the `kill_handle`
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
    /// Abort capability mailbox (send side). The managing blox sends
    /// `AbortCommand` here; the child's task receives it and self-terminates
    /// cooperatively (no callbacks, no dispatch).
    pub abort_ref: ActorRef<AbortCommand, R>,
    /// Cloneable kill handle for external task kill (the ripcord).
    /// `()` for NoKill runtimes, `R::KillHandle` for Kill runtimes.
    /// Must be `Clone` so the action function can extract it from `&Event`.
    pub kill_handle: <R::Kill as KillCapability<R>>::Handle,
    pub policy: ChildPolicy,
}

impl<R: BloxRuntime> Clone for RegisterDynamicChild<R> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            lifecycle_ref: self.lifecycle_ref.clone(),
            abort_ref: self.abort_ref.clone(),
            kill_handle: self.kill_handle.clone(),
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

/// Child-management control-plane messages delivered through a dedicated
/// control mailbox.
///
/// There is no `Spawn` variant — spawning is decoupled from the managing blox.
/// The spawn helper calls `spawn_dynamic_child()` (in `bloxide-spawn`) which sends
/// `RegisterDynamicChild` on the control mailbox after the child is created.
///
/// Implements `Clone` because all variants are `Clone` (`RegisterDynamicChild`
/// uses `kill_handle` which is `Clone`). Manual impl (not `#[derive]`) to
/// avoid the derive macro generating `R: Clone` bounds.
pub enum ChildCtrl<R: BloxRuntime> {
    /// Register a static child (wired at startup, no abort capability).
    RegisterChild(RegisterChild<R>),
    /// Register a dynamically spawned child (has abort capability + kill handle).
    RegisterDynamicChild(RegisterDynamicChild<R>),
    /// Trigger one health-check round.
    WatchdogTick,
}

impl<R: BloxRuntime> Clone for ChildCtrl<R> {
    fn clone(&self) -> Self {
        match self {
            Self::RegisterChild(r) => Self::RegisterChild(r.clone()),
            Self::RegisterDynamicChild(r) => Self::RegisterDynamicChild(r.clone()),
            Self::WatchdogTick => Self::WatchdogTick,
        }
    }
}

impl<R: BloxRuntime> fmt::Debug for ChildCtrl<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegisterChild(r) => f.debug_tuple("RegisterChild").field(r).finish(),
            Self::RegisterDynamicChild(r) => {
                f.debug_tuple("RegisterDynamicChild").field(r).finish()
            }
            Self::WatchdogTick => write!(f, "WatchdogTick"),
        }
    }
}
