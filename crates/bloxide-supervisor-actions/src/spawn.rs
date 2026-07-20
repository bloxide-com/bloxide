// Copyright 2025 Bloxide, all rights reserved
//! Dynamic child registration action.
//!
//! The old `SpawnFactory`/`HasSpawnFactory` pattern was removed in spec 22.
//! Spawning is now decoupled from the supervisor — the requesting blox calls
//! `bloxide_core::spawn::spawn_child` directly, and the supervisor receives
//! `RegisterDynamicChild` via its control channel. This action handles that
//! registration message.

use bloxide_core::{accessor::HasSelfId, capability::BloxRuntime, transition::ActionResult};
use bloxide_supervisor_context::{HasChildGroupMut, SupervisorControl, SupervisorEventLike};

/// Handle a `RegisterDynamicChild` control message.
///
/// Called when the supervisor receives a `SupervisorControl::RegisterDynamicChild`
/// from the `spawn_child` helper. Registers the child in the child group and
/// sends a Start command.
pub fn handle_register_dynamic_child<R, C, E>(ctx: &mut C, ev: &E) -> ActionResult
where
    R: BloxRuntime,
    C: HasSelfId + HasChildGroupMut<R>,
    E: SupervisorEventLike<R>,
{
    if let Some(SupervisorControl::RegisterDynamicChild(reg)) = ev.as_control_event() {
        let from = ctx.self_id();
        let child_id = reg.id;
        ctx.children_mut()
            .add(child_id, reg.lifecycle_ref.clone(), reg.policy);
        ctx.children_mut().start_child(child_id, from);
    }
    ActionResult::Ok
}

/// Backward-compat alias for generated code that may still reference
/// `handle_spawn_request`.
pub fn handle_spawn_request<R, C, E>(ctx: &mut C, ev: &E) -> ActionResult
where
    R: BloxRuntime,
    C: HasSelfId + HasChildGroupMut<R>,
    E: SupervisorEventLike<R>,
{
    handle_register_dynamic_child(ctx, ev)
}
