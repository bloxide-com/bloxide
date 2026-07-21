// Copyright 2025 Bloxide, all rights reserved
//! Child notify accessor trait.
//!
//! The managing blox (supervisor or custom) needs a reference to the
//! child-event mailbox so spawned children can report lifecycle events back.
//! Spawning is decoupled from the supervisor — see `bloxide_spawn`
//! for `SpawnOutput`, `ChildRegistrar`, and the `spawn_child` generic helper.

use bloxide_core::{capability::BloxRuntime, lifecycle::ChildLifecycleEvent, messaging::ActorRef};

/// Accessor trait for the child notify channel.
pub trait HasChildNotify<R: BloxRuntime> {
    fn child_notify(&self) -> &ActorRef<ChildLifecycleEvent, R>;
}
