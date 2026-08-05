// Copyright 2025 Bloxide, all rights reserved
//! Child registration — the fallible entry-creation surface of `ChildGroup`.
//!
//! Registration is fallible everywhere — the managing blox's action
//! functions warn-and-drop on `Err` instead of panicking (a bad `ChildCtrl`
//! message must not reset an MCU).

use bloxide_core::{
    capability::{BloxRuntime, KillCapability},
    lifecycle::{AbortCommand, LifecycleCommand},
    messaging::{ActorId, ActorRef},
};

use crate::group::{ChildEntry, ChildGroup, ChildPhase};
use crate::policy::ChildPolicy;

/// Why a registration was rejected. Registration is fallible everywhere —
/// the managing blox's action functions warn-and-drop on `Err` instead of
/// panicking (a bad `ChildCtrl` message must not reset an MCU).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RegistrationError {
    /// `ChildPolicy::Abort`/`Kill` requested for a static child (no abort
    /// mailbox, no kill handle). Register via `try_add_dynamic` or use
    /// `Reset`/`Stop`.
    PolicyRequiresHandles,
    /// `ChildPolicy::Kill` on a runtime whose `KillCapability` is a no-op
    /// (`!CAN_KILL`, e.g. Embassy). Killing would not work and the group
    /// would wrongly mark a live child dead.
    KillUnavailable,
    /// A child with this `ActorId` is already registered.
    Duplicate,
}

impl<R: BloxRuntime> ChildGroup<R> {
    /// Register a static child (no abort/kill capability).
    ///
    /// Fallible: `Abort`/`Kill` policies need handles (`try_add_dynamic`),
    /// and duplicate ids are rejected.
    pub fn try_add(
        &mut self,
        id: ActorId,
        lifecycle_ref: ActorRef<LifecycleCommand, R>,
        policy: ChildPolicy,
    ) -> Result<(), RegistrationError> {
        if matches!(policy, ChildPolicy::Kill | ChildPolicy::Abort) {
            return Err(RegistrationError::PolicyRequiresHandles);
        }
        if self.children.iter().any(|e| e.id == id) {
            return Err(RegistrationError::Duplicate);
        }
        self.children.push(ChildEntry {
            id,
            lifecycle_ref,
            policy,
            phase: ChildPhase::Init,
            ping_outstanding: false,
            misses: 0,
            restarts: 0,
            pending_cmd: None,
            abort_ref: None,
            kill_handle: None,
        });
        Ok(())
    }

    /// Register a dynamically spawned child that has an abort capability.
    ///
    /// Stores the `abort_ref` (for cooperative self-termination via the abort
    /// mailbox) and the `kill_handle` (for the external kill ripcord) so the
    /// supervisor can abort or kill the child when policy dictates.
    ///
    /// Fallible: `ChildPolicy::Kill` requires `KillCapability::CAN_KILL`
    /// (refused on `NoKill` runtimes — the kill would be a no-op and the
    /// group would mark a live child dead), and duplicate ids are rejected.
    pub fn try_add_dynamic(
        &mut self,
        id: ActorId,
        lifecycle_ref: ActorRef<LifecycleCommand, R>,
        abort_ref: ActorRef<AbortCommand, R>,
        kill_handle: <R::Kill as KillCapability<R>>::Handle,
        policy: ChildPolicy,
    ) -> Result<(), RegistrationError> {
        if policy == ChildPolicy::Kill && !<R::Kill as KillCapability<R>>::CAN_KILL {
            return Err(RegistrationError::KillUnavailable);
        }
        if self.children.iter().any(|e| e.id == id) {
            return Err(RegistrationError::Duplicate);
        }
        self.children.push(ChildEntry {
            id,
            lifecycle_ref,
            policy,
            phase: ChildPhase::Init,
            ping_outstanding: false,
            misses: 0,
            restarts: 0,
            pending_cmd: None,
            abort_ref: Some(abort_ref),
            kill_handle: Some(kill_handle),
        });
        Ok(())
    }
}
