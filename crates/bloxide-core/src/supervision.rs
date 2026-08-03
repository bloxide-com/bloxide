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
/// Supervision must never block the actor's run loop: a full channel drops
/// the event and logs a warning (genuine backpressure), while a closed
/// channel — the supervisor already exited, an expected shutdown race —
/// drops the event silently.
///
/// # Type Parameters
///
/// * `S` — The actor's [`MachineSpec`]. Used to check `is_error`
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
        match <R as BloxRuntime>::try_send_via(notify, Envelope(actor_id, event)) {
            Ok(()) => {}
            // Expected shutdown race: the supervisor's task already exited —
            // nobody consumes the report. Silent no-op.
            Err(e) if R::try_send_error_is_closed(&e) => {}
            Err(_) => {
                bloxide_log::blox_log_warn!(
                    actor_id,
                    "failed to send lifecycle event to supervisor (channel full)"
                );
            }
        }
    };

    match outcome {
        DispatchOutcome::Started(MachineState::State(s)) => {
            if S::is_error(s) {
                send(ChildLifecycleEvent::Failed { child_id: actor_id });
            } else {
                send(ChildLifecycleEvent::Started { child_id: actor_id });
            }
        }
        DispatchOutcome::Transition(MachineState::State(_)) => {
            // Transitions into an error state are converted to
            // `DispatchOutcome::Failed` by dispatch() before reaching here,
            // so there is nothing to report for ordinary transitions.
        }
        DispatchOutcome::Failed => {
            send(ChildLifecycleEvent::Failed { child_id: actor_id });
        }
        DispatchOutcome::Stopped => {
            send(ChildLifecycleEvent::Stopped { child_id: actor_id });
        }
        DispatchOutcome::Done => {
            send(ChildLifecycleEvent::Done { child_id: actor_id });
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
