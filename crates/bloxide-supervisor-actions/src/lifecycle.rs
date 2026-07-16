// Copyright 2025 Bloxide, all rights reserved
//! Lifecycle action functions for the supervisor state machine.
//!
//! These are generic over any context `C` that implements the required
//! accessor traits and any event type `E` implementing `SupervisorEventLike`.

use bloxide_core::{accessor::HasSelfId, lifecycle::ChildLifecycleEvent, transition::ActionResult};
use bloxide_supervisor_context::{
    HasChildGroup, HasChildGroupMut, HasPending, SupervisorControl, SupervisorEventLike,
};

/// Start all children in the group.
pub fn start_children<R, C>(ctx: &mut C)
where
    R: bloxide_core::capability::BloxRuntime,
    C: HasSelfId + HasChildGroup<R>,
{
    ctx.children().start_all(ctx.self_id());
}

/// Stop all children in the group.
pub fn stop_all_children<R, C>(ctx: &mut C)
where
    R: bloxide_core::capability::BloxRuntime,
    C: HasSelfId + HasChildGroup<R>,
{
    ctx.children().stop_all(ctx.self_id());
}

/// Handle a Done or Failed child lifecycle event.
pub fn handle_done_or_failed<R, C, E>(ctx: &mut C, ev: &E) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
    C: HasSelfId + HasChildGroupMut<R> + HasPending,
    E: SupervisorEventLike<R>,
{
    if let Some(ChildLifecycleEvent::Done { child_id } | ChildLifecycleEvent::Failed { child_id }) =
        ev.as_child_event()
    {
        let from = ctx.self_id();
        let action = ctx.children_mut().handle_done_or_failed(*child_id, from);
        ctx.set_pending(action);
    }
    ActionResult::Ok
}

/// Handle a Reset child lifecycle event.
pub fn handle_reset<R, C, E>(ctx: &mut C, ev: &E) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
    C: HasSelfId + HasChildGroupMut<R>,
    E: SupervisorEventLike<R>,
{
    if let Some(ChildLifecycleEvent::Reset { child_id }) = ev.as_child_event() {
        let from = ctx.self_id();
        ctx.children_mut().handle_reset(*child_id, from);
    }
    ActionResult::Ok
}

/// Record a stopped child.
pub fn record_stopped<R, C, E>(ctx: &mut C, ev: &E) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
    C: HasChildGroupMut<R>,
    E: SupervisorEventLike<R>,
{
    if let Some(ChildLifecycleEvent::Stopped { child_id }) = ev.as_child_event() {
        ctx.children_mut().record_stopped(*child_id);
    }
    ActionResult::Ok
}

/// Record a started child.
pub fn record_started<R, C, E>(ctx: &mut C, ev: &E) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
    C: HasChildGroupMut<R>,
    E: SupervisorEventLike<R>,
{
    if let Some(ChildLifecycleEvent::Started { child_id }) = ev.as_child_event() {
        ctx.children_mut().handle_started(*child_id);
    }
    ActionResult::Ok
}

/// Record an alive child.
pub fn record_alive<R, C, E>(ctx: &mut C, ev: &E) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
    C: HasChildGroupMut<R>,
    E: SupervisorEventLike<R>,
{
    if let Some(ChildLifecycleEvent::Alive { child_id }) = ev.as_child_event() {
        ctx.children_mut().handle_alive(*child_id);
    }
    ActionResult::Ok
}

/// Register a new child dynamically.
pub fn register_child<R, C, E>(ctx: &mut C, ev: &E) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
    C: HasSelfId + HasChildGroupMut<R>,
    E: SupervisorEventLike<R>,
{
    if let Some(SupervisorControl::RegisterChild(child)) = ev.as_control_event() {
        let from = ctx.self_id();
        let (id, lifecycle_ref, policy) = (child.id, child.lifecycle_ref.clone(), child.policy);
        ctx.children_mut().add(id, lifecycle_ref, policy);
        ctx.children_mut().start_child(id, from);
    }
    ActionResult::Ok
}

/// Handle a health-check tick.
pub fn handle_health_check<R, C, E>(ctx: &mut C, ev: &E) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
    C: HasSelfId + HasChildGroupMut<R> + HasPending,
    E: SupervisorEventLike<R>,
{
    if let Some(SupervisorControl::HealthCheckTick) = ev.as_control_event() {
        let from = ctx.self_id();
        let action = ctx.children_mut().health_check_tick(from);
        ctx.set_pending(action);
    }
    ActionResult::Ok
}
