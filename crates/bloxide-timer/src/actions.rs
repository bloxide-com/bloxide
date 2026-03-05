use bloxide_core::{accessor::HasSelfId, capability::BloxRuntime, messaging::ActorRef};

use crate::command::{next_timer_id, TimerCommand, TimerId, TIMER_ACTOR_ID};

/// Accessor trait for blox contexts that hold a timer service reference.
///
/// Implement via `#[provides(HasTimerRef<R>)]` on a `timer_ref` field
/// in a `#[derive(BloxCtx)]` context struct.
pub trait HasTimerRef<R: BloxRuntime> {
    fn timer_ref(&self) -> &ActorRef<TimerCommand, R>;
}

/// Schedule `event` to be delivered to `target` after `after_ms` milliseconds.
///
/// Returns the `TimerId` that can be passed to `cancel_timer` later.
/// Logs a warning if the timer channel is full and the command was dropped.
pub fn set_timer<R, C, M>(ctx: &C, after_ms: u64, target: &ActorRef<M, R>, event: M) -> TimerId
where
    R: BloxRuntime,
    C: HasSelfId + HasTimerRef<R>,
    M: Send + 'static,
{
    let id = next_timer_id();
    let target = target.clone();
    let deliver = alloc::boxed::Box::new(move || {
        let _ = target.try_send(TIMER_ACTOR_ID, event);
    });
    if ctx
        .timer_ref()
        .try_send(
            ctx.self_id(),
            TimerCommand::Set {
                id,
                after_ms,
                deliver,
            },
        )
        .is_err()
    {
        bloxide_log::blox_log_warn!(
            ctx.self_id(),
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
pub fn cancel_timer<R, C>(ctx: &C, id: TimerId)
where
    R: BloxRuntime,
    C: HasSelfId + HasTimerRef<R>,
{
    if ctx
        .timer_ref()
        .try_send(ctx.self_id(), TimerCommand::Cancel { id })
        .is_err()
    {
        bloxide_log::blox_log_warn!(
            ctx.self_id(),
            "cancel_timer: timer channel full, cancel for timer {} dropped — it may still fire",
            id.as_u64()
        );
    }
}
