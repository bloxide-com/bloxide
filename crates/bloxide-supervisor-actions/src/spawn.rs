// Copyright 2025 Bloxide, all rights reserved
//! Spawn action functions — only available with the `dynamic` feature.

#![cfg(feature = "dynamic")]

use bloxide_core::{accessor::HasSelfId, capability::BloxRuntime, transition::ActionResult};
use bloxide_supervisor_context::{
    ChildPolicy, HasChildGroupMut, HasChildNotify, HasSpawnFactory, SpawnFactory, SpawnPolicy,
    SupervisorEvent,
};

/// Convert an optional `SpawnPolicy` into a `ChildPolicy`.
fn to_child_policy(policy: Option<SpawnPolicy>) -> ChildPolicy {
    match policy {
        Some(SpawnPolicy::Restart { max }) => ChildPolicy::Restart { max },
        Some(SpawnPolicy::Stop) => ChildPolicy::Stop,
        Some(SpawnPolicy::Kill) => ChildPolicy::Kill,
        None => ChildPolicy::Stop,
    }
}

/// Handle a spawn request from a client.
///
/// Calls the factory to create a child actor, registers it with the
/// supervisor's child group, and sends a Start command.
pub fn handle_spawn_request<R, C, F>(ctx: &mut C, ev: &SupervisorEvent<R, F>) -> ActionResult
where
    R: BloxRuntime,
    C: HasSelfId + HasChildGroupMut<R> + HasChildNotify<R> + HasSpawnFactory<R, Factory = F>,
    F: SpawnFactory<R>,
{
    if let SupervisorEvent::Spawn(request) = ev {
        let from = ctx.self_id();
        let notify = ctx.child_notify().clone();
        let output = ctx.spawn_factory().spawn(request.clone(), notify);
        let child_id = output.child_id;
        ctx.children_mut().add(
            child_id,
            output.lifecycle_ref,
            to_child_policy(output.policy),
        );
        ctx.children_mut().start_child(child_id, from);
    }
    ActionResult::Ok
}
