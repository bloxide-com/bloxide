// Copyright 2025 Bloxide, all rights reserved
//! Shared supervision reporting logic.
//!
//! [`report_outcome`] translates a [`DispatchOutcome`] into a
//! [`ChildLifecycleEvent`] and sends it to the supervisor's notify channel.
//! Both the Embassy and Tokio runtimes call this function — it is generic over
//! the runtime (`R: BloxRuntime`) so each runtime supplies its own sender type.

use crate::capability::BloxRuntime;
use crate::engine::{DispatchOutcome, MachineState};
use crate::lifecycle::ChildLifecycleEvent;
use crate::messaging::{ActorId, Envelope};
use crate::spec::MachineSpec;

/// Translate a `DispatchOutcome` into the appropriate `ChildLifecycleEvent`
/// and send it to the supervisor via `notify`.
///
/// If the supervisor's channel is full or closed, the event is silently
/// dropped and a warning is logged — supervision must never block the actor's
/// run loop.
///
/// # Type Parameters
///
/// * `S` — The actor's [`MachineSpec`]. Used to check `is_error` / `is_terminal`
///   on the resulting state.
/// * `R` — The [`BloxRuntime`], which determines the concrete sender type
///   (`R::Sender<ChildLifecycleEvent>`).
pub fn report_outcome<S, R>(
    outcome: &DispatchOutcome<S::State>,
    actor_id: ActorId,
    notify: &R::Sender<ChildLifecycleEvent>,
) where
    S: MachineSpec,
    R: BloxRuntime,
{
    let send = |event| {
        if <R as BloxRuntime>::try_send_via(notify, Envelope(actor_id, event)).is_err() {
            bloxide_log::blox_log_warn!(
                actor_id,
                "failed to send lifecycle event to supervisor (channel full or closed)"
            );
        }
    };

    match outcome {
        DispatchOutcome::Started(MachineState::State(s)) => {
            if S::is_error(s) {
                send(ChildLifecycleEvent::Failed { child_id: actor_id });
            } else if S::is_terminal(s) {
                send(ChildLifecycleEvent::Done { child_id: actor_id });
            } else {
                send(ChildLifecycleEvent::Started { child_id: actor_id });
            }
        }
        DispatchOutcome::Transition(MachineState::State(s)) => {
            if S::is_error(s) {
                send(ChildLifecycleEvent::Failed { child_id: actor_id });
            } else if S::is_terminal(s) {
                send(ChildLifecycleEvent::Done { child_id: actor_id });
            }
        }
        DispatchOutcome::Done(MachineState::State(_)) => {
            send(ChildLifecycleEvent::Done { child_id: actor_id });
        }
        DispatchOutcome::Failed => {
            send(ChildLifecycleEvent::Failed { child_id: actor_id });
        }
        DispatchOutcome::Stopped => {
            send(ChildLifecycleEvent::Stopped { child_id: actor_id });
        }
        DispatchOutcome::Aborted => {
            send(ChildLifecycleEvent::Aborted { child_id: actor_id });
        }
        DispatchOutcome::Alive => {
            send(ChildLifecycleEvent::Alive { child_id: actor_id });
        }
        _ => {}
    }
}
