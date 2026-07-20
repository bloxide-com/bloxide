// Copyright 2025 Bloxide, all rights reserved
//! Child notify accessor and spawn convenience wrapper.
//!
//! The managing blox (supervisor or custom) needs a reference to the
//! child-event mailbox so spawned children can report lifecycle events back.
//! Spawning is decoupled from the supervisor — see `bloxide_core::spawn`
//! for `SpawnOutput`, `ChildRegistrar`, and the `spawn_child` generic helper.

use bloxide_core::{
    capability::BloxRuntime, lifecycle::ChildLifecycleEvent, messaging::ActorRef, spawn::SpawnFn,
};

use crate::control::{SupervisorControl, SupervisorRegistrar};

/// Accessor trait for the child notify channel.
pub trait HasChildNotify<R: BloxRuntime> {
    fn child_notify(&self) -> &ActorRef<ChildLifecycleEvent, R>;
}

/// Convenience wrapper around [`bloxide_core::spawn::spawn_child`] that fixes
/// `C = SupervisorRegistrar` — i.e. the standard supervisor is the managing
/// blox.
///
/// Called by the requesting blox (e.g., the Pool) — NOT by the supervisor.
/// The requesting blox provides the spawn function and the request; this
/// helper calls the spawn function and sends `RegisterDynamicChild` to the
/// supervisor's control mailbox.
///
/// See `spec/architecture/22-spawn-architecture-v2.md` §4.11.
pub fn spawn_supervised_child<R, Req>(
    spawn_fn: SpawnFn<R, Req>,
    req: Req,
    control_ref: &ActorRef<SupervisorControl<R>, R>,
    notify_ref: &ActorRef<ChildLifecycleEvent, R>,
    from: bloxide_core::messaging::ActorId,
) -> Result<(), R::TrySendError>
where
    R: BloxRuntime,
    Req: Send + Clone + 'static,
{
    bloxide_core::spawn::spawn_child::<R, Req, SupervisorRegistrar>(
        spawn_fn,
        req,
        control_ref,
        notify_ref,
        from,
    )
}
