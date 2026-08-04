// Copyright 2025 Bloxide, all rights reserved
//! Lifecycle action functions for a managing blox (e.g. the standard supervisor).
//!
//! These take concrete params (extracted from the context by the generated
//! wrapper closures) plus an extracted event payload — `&ChildLifecycleEvent`
//! or `&ChildCtrl<R>` — never the consumer's event enum type (spec 20:
//! Platform Feature Pattern).

use crate::control::ChildCtrl;
use crate::{ChildAction, ChildGroup};
use bloxide_core::{lifecycle::ChildLifecycleEvent, messaging::ActorRef, transition::ActionResult};

/// Start all children in the group and clear lifecycle counters.
///
/// This is the `on_entry` for the managing blox's Running state. In the
/// four-level lifecycle model, `Decision::Reset` goes directly to
/// `initial_state()` (Running) — it does NOT fire `on_init_entry`. So
/// counters must be cleared here, in the Running on_entry, which fires both
/// on initial Start and on Reset.
pub fn start_children<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    pending: &mut ChildAction,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    children.clear_counters();
    *pending = ChildAction::default();
    children.start_all(self_id);
    ActionResult::Ok
}

/// Stop all children in the group.
pub fn stop_all_children<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    children.stop_all(self_id);
    ActionResult::Ok
}

/// Retry every queued undelivered command (confirm-before-record).
///
/// Wired as the first action on the managing blox's transitions so a command
/// lost to a full channel is retried on every event pass. The group shutdown
/// decision is written to `pending` — a flush can complete a shutdown (a
/// Closed channel observed here marks the child `Gone`).
pub fn flush_pending<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    pending: &mut ChildAction,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    *pending = children.flush_pending(self_id);
    ActionResult::Ok
}

/// Handle a Stopped or Failed child lifecycle event.
///
/// Serves both the `Stopped` and `Failed` transition rules — the extracted
/// payload is matched internally. Applies the child's `ChildPolicy` via
/// `ChildGroup::handle_done_or_failed`.
pub fn handle_done_or_failed<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    child_notify: &ActorRef<ChildLifecycleEvent, R>,
    pending: &mut ChildAction,
    ev: &ChildLifecycleEvent,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildLifecycleEvent::Stopped { child_id } | ChildLifecycleEvent::Failed { child_id } = ev
    {
        let action = children.handle_done_or_failed(*child_id, self_id, child_notify);
        *pending = action;
    }
    ActionResult::Ok
}

/// Record a started child.
///
/// In the four-level lifecycle model, `Started` covers both initial `Start`
/// and `Reset` (both go directly to `initial_state()`). The managing blox
/// does not need to send `Start` after `Reset`.
pub fn record_started<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    ev: &ChildLifecycleEvent,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildLifecycleEvent::Started { child_id } = ev {
        children.handle_started(*child_id, self_id);
    }
    ActionResult::Ok
}

/// Record a stopped child (ShuttingDown accounting).
pub fn record_stopped<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    ev: &ChildLifecycleEvent,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildLifecycleEvent::Stopped { child_id } = ev {
        children.record_stopped(*child_id, self_id);
    }
    ActionResult::Ok
}

/// Record a failed child received while shutting down.
///
/// The child is parked in its absorbing error state (task alive) — recorded
/// as terminal for shutdown accounting. (In Running, `Failed` goes through
/// `handle_done_or_failed` instead, applying the child policy.)
pub fn record_failed<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    ev: &ChildLifecycleEvent,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildLifecycleEvent::Failed { child_id } = ev {
        children.record_failed(*child_id, self_id);
    }
    ActionResult::Ok
}

/// Record an aborted child.
///
/// Aborted means the child's task self-terminated cooperatively via
/// `AbortCommand`. The task is gone — restarting requires respawning.
/// Externally-originated aborts participate in group shutdown: the decision
/// is written to `pending`.
pub fn record_aborted<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    pending: &mut ChildAction,
    ev: &ChildLifecycleEvent,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildLifecycleEvent::Aborted { child_id } = ev {
        *pending = children.record_aborted(*child_id, self_id);
    }
    ActionResult::Ok
}

/// Record a killed child.
///
/// Killed means the child's task was destroyed externally via
/// `KillCapability::kill(handle)`. Permanently dead — cannot be restarted
/// without respawning the task. Participates in group shutdown: the decision
/// is written to `pending`.
pub fn record_killed<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    pending: &mut ChildAction,
    ev: &ChildLifecycleEvent,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildLifecycleEvent::Killed { child_id } = ev {
        *pending = children.record_killed(*child_id, self_id);
    }
    ActionResult::Ok
}

/// Record an alive child.
pub fn record_alive<R>(children: &mut ChildGroup<R>, ev: &ChildLifecycleEvent) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildLifecycleEvent::Alive { child_id } = ev {
        children.handle_alive(*child_id);
    }
    ActionResult::Ok
}

/// Deregister a child that self-terminated cleanly (`ChildLifecycleEvent::Done`).
///
/// Done is normal completion: the entry is removed (no restart policy), and
/// the group shutdown decision is recorded in `pending` so the managing blox
/// can still progress to shutdown when the last child completes. A `Done`
/// from an unknown child is ignored (and never triggers shutdown).
pub fn deregister_done<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    pending: &mut ChildAction,
    ev: &ChildLifecycleEvent,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildLifecycleEvent::Done { child_id } = ev {
        *pending = children.deregister(*child_id, self_id);
    }
    ActionResult::Ok
}

/// Register a new static child.
///
/// Fallible: an invalid policy (`Abort`/`Kill` need handles) or a duplicate
/// id warns and drops the registration — a malformed control message must
/// not panic the managing blox.
pub fn register_child<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    ctrl: &ChildCtrl<R>,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildCtrl::RegisterChild(child) = ctrl {
        let (id, lifecycle_ref, policy) = (child.id, child.lifecycle_ref.clone(), child.policy);
        match children.try_add(id, lifecycle_ref, policy) {
            Ok(()) => children.start_child(id, self_id),
            Err(err) => {
                bloxide_log::blox_log_warn!(
                    self_id,
                    "RegisterChild for {} rejected ({:?}) — registration dropped",
                    id,
                    err
                );
            }
        }
    }
    ActionResult::Ok
}

/// Handle a `RegisterDynamicChild` control message.
///
/// Called when the managing blox receives a `ChildCtrl::RegisterDynamicChild`
/// from the `spawn_child` helper. Registers the child in the child group
/// (storing the `abort_ref` for the cooperative abort mailbox and the
/// `kill_handle` for the external kill ripcord) and sends a Start command.
///
/// Fallible, like `register_child`: `ChildPolicy::Kill` on a `!CAN_KILL`
/// runtime or a duplicate id warns and drops the registration.
pub fn handle_register_dynamic_child<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    ctrl: &ChildCtrl<R>,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildCtrl::RegisterDynamicChild(reg) = ctrl {
        let child_id = reg.id;
        match children.try_add_dynamic(
            child_id,
            reg.lifecycle_ref.clone(),
            reg.abort_ref.clone(),
            reg.kill_handle.clone(),
            reg.policy,
        ) {
            Ok(()) => children.start_child(child_id, self_id),
            Err(err) => {
                bloxide_log::blox_log_warn!(
                    self_id,
                    "RegisterDynamicChild for {} rejected ({:?}) — registration dropped",
                    child_id,
                    err
                );
            }
        }
    }
    ActionResult::Ok
}

/// Handle a watchdog tick (drives one health-check round).
pub fn handle_watchdog_tick<R>(
    self_id: bloxide_core::ActorId,
    children: &mut ChildGroup<R>,
    child_notify: &ActorRef<ChildLifecycleEvent, R>,
    pending: &mut ChildAction,
    ctrl: &ChildCtrl<R>,
) -> ActionResult
where
    R: bloxide_core::capability::BloxRuntime,
{
    if let ChildCtrl::WatchdogTick = ctrl {
        let action = children.watchdog_tick(self_id, child_notify);
        *pending = action;
    }
    ActionResult::Ok
}
