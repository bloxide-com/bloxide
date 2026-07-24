// Copyright 2025 Bloxide, all rights reserved
//! Lifecycle action functions for the supervisor state machine.
//!
//! These take concrete params (extracted from the context by the generated
//! wrapper closures) rather than `&mut SupervisorCtx<R>` + accessor traits.
//! The event type is `&SupervisorEvent<R>` — the codegen passes it via the
//! `event_arg` mechanism.

use crate::SupervisorControl;
use bloxide_child_management::{ChildAction, ChildGroup};
use bloxide_core::{
    lifecycle::ChildLifecycleEvent, messaging::ActorRef, messaging::Envelope,
    transition::ActionResult,
};

use crate::SupervisorEvent;

/// Start all children in the group and clear lifecycle counters.
///
/// This is the `on_entry` for the Running state. In the four-level lifecycle
/// model, `Guard::Reset` goes directly to `initial_state()` (Running) — it
/// does NOT fire `on_init_entry`. So counters must be cleared here, in the
/// Running on_entry, which fires both on initial Start and on Reset.
pub fn start_children<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    pending: &mut ChildAction,
) where
    R: bloxide_core::capability::BloxRuntime,
{
    children.clear_counters();
    *pending = ChildAction::default();
    children.start_all(self_id);
}

/// Stop all children in the group.
pub fn stop_all_children<R>(self_id: bloxide_core::ActorId, children: &ChildGroup<R>)
where
    R: bloxide_core::capability::BloxRuntime,
{
    children.stop_all(self_id);
}

/// Handle a Stopped or Failed child lifecycle event.
pub fn handle_done_or_failed<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    child_notify: &ActorRef<ChildLifecycleEvent, R>,
    pending: &mut ChildAction,
    ev: &SupervisorEvent<R>,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { child_id }))
    | SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Failed { child_id })) = ev
    {
        let action = children.handle_done_or_failed(*child_id, self_id, child_notify);
        *pending = action;
    }
    ActionResult::Ok
}

/// Record a started child.
///
/// In the four-level lifecycle model, `Started` covers both initial `Start`
/// and `Reset` (both go directly to `initial_state()`). The supervisor does
/// not need to send `Start` after `Reset`.
pub fn record_started<R>(children: &mut ChildGroup<R>, ev: &SupervisorEvent<R>) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Started { child_id })) = ev {
        children.handle_started(*child_id);
    }
    ActionResult::Ok
}

/// Record a stopped child.
pub fn record_stopped<R>(children: &mut ChildGroup<R>, ev: &SupervisorEvent<R>) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { child_id })) = ev {
        children.record_stopped(*child_id);
    }
    ActionResult::Ok
}

/// Record an aborted child.
///
/// Aborted means the child's task self-terminated cooperatively via
/// `AbortCommand`. The task is gone — restarting requires respawning.
pub fn record_aborted<R>(children: &mut ChildGroup<R>, ev: &SupervisorEvent<R>) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Aborted { child_id })) = ev {
        children.record_aborted(*child_id);
    }
    ActionResult::Ok
}

/// Record a killed child.
///
/// Killed means the child's task was destroyed externally via
/// `KillCapability::kill(handle)`. Permanently dead — cannot be restarted
/// without respawning the task.
pub fn record_killed<R>(children: &mut ChildGroup<R>, ev: &SupervisorEvent<R>) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Killed { child_id })) = ev {
        children.record_killed(*child_id);
    }
    ActionResult::Ok
}

/// Record an alive child.
pub fn record_alive<R>(children: &mut ChildGroup<R>, ev: &SupervisorEvent<R>) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Alive { child_id })) = ev {
        children.handle_alive(*child_id);
    }
    ActionResult::Ok
}

/// Register a new static child.
pub fn register_child<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    ev: &SupervisorEvent<R>,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let SupervisorEvent::Control(Envelope(_, SupervisorControl::RegisterChild(child))) = ev {
        let (id, lifecycle_ref, policy) = (child.id, child.lifecycle_ref.clone(), child.policy);
        children.add(id, lifecycle_ref, policy);
        children.start_child(id, self_id);
    }
    ActionResult::Ok
}

/// Handle a `RegisterDynamicChild` control message.
///
/// Called when the supervisor receives a `SupervisorControl::RegisterDynamicChild`
/// from the `spawn_child` helper. Registers the child in the child group
/// (storing the `abort_ref` for the cooperative abort mailbox and the
/// `kill_handle` for the external kill ripcord) and sends a Start command.
pub fn handle_register_dynamic_child<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    ev: &SupervisorEvent<R>,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let SupervisorEvent::Control(Envelope(_, SupervisorControl::RegisterDynamicChild(reg))) = ev
    {
        let child_id = reg.id;
        children.add_dynamic(
            child_id,
            reg.lifecycle_ref.clone(),
            reg.abort_ref.clone(),
            reg.kill_handle.clone(),
            reg.policy,
        );
        children.start_child(child_id, self_id);
    }
    ActionResult::Ok
}

/// Handle a health-check tick.
pub fn handle_health_check<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    child_notify: &ActorRef<ChildLifecycleEvent, R>,
    pending: &mut ChildAction,
    ev: &SupervisorEvent<R>,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let SupervisorEvent::Control(Envelope(_, SupervisorControl::HealthCheckTick)) = ev {
        let action = children.health_check_tick(self_id, child_notify);
        *pending = action;
    }
    ActionResult::Ok
}
