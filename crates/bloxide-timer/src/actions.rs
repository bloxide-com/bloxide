// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{capability::BloxRuntime, messaging::ActorId, messaging::ActorRef};

use crate::command::{next_timer_id, TimerCommand, TimerId, TIMER_ACTOR_ID};

/// Schedule `event` to be delivered to `target` after `after_ms` milliseconds.
///
/// Returns the `TimerId` that can be passed to `cancel_timer` later.
/// Logs a warning if the timer channel is full and the command was dropped.
pub fn set_timer<R, M>(
    self_id: ActorId,
    timer_ref: &ActorRef<TimerCommand, R>,
    after_ms: u64,
    target: &ActorRef<M, R>,
    event: M,
) -> TimerId
where
    R: BloxRuntime,
    M: Send + 'static,
{
    let id = next_timer_id();
    let target = target.clone();
    let deliver = alloc::boxed::Box::new(move || {
        if target.try_send(TIMER_ACTOR_ID, event).is_err() {
            bloxide_log::blox_log_warn!(
                self_id,
                "timer delivery: target mailbox full, timer event dropped"
            );
        }
    });
    if timer_ref
        .try_send(
            self_id,
            TimerCommand::Set {
                id,
                after_ms,
                deliver,
            },
        )
        .is_err()
    {
        bloxide_log::blox_log_warn!(
            self_id,
            "set_timer: timer channel full, timer {} dropped — it will never fire",
            id.as_u64()
        );
    }
    id
}

/// Cancel a previously scheduled timer.
///
/// Logs a warning if the timer channel is full and the cancel command was dropped
/// (the timer may still fire).
pub fn cancel_timer<R>(self_id: ActorId, timer_ref: &ActorRef<TimerCommand, R>, id: TimerId)
where
    R: BloxRuntime,
{
    if timer_ref
        .try_send(self_id, TimerCommand::Cancel { id })
        .is_err()
    {
        bloxide_log::blox_log_warn!(
            self_id,
            "cancel_timer: timer channel full, cancel for timer {} dropped — it may still fire",
            id.as_u64()
        );
    }
}

/// Cancel the stored timer (if any) and clear the slot.
///
/// The `Option<TimerId>` bookkeeping pattern: a context keeps at most one
/// current timer in a plain field; this cancels it and clears the field in
/// one call.
pub fn cancel_timer_by_id<R>(
    self_id: ActorId,
    timer_ref: &ActorRef<TimerCommand, R>,
    current_timer: &mut Option<TimerId>,
) where
    R: BloxRuntime,
{
    if let Some(id) = current_timer.take() {
        cancel_timer(self_id, timer_ref, id);
    }
}
