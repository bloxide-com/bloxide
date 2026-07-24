// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for the current-timer-id behavior.
//!
//! Provides action functions for scheduling and cancelling resume timers.
//! The `current_timer` field is now a plain struct field on the context.
#![no_std]

extern crate alloc;

use bloxide_core::{capability::BloxRuntime, messaging::ActorRef, ActorId};
use bloxide_timer::command::{next_timer_id, TimerCommand, TimerId, TIMER_ACTOR_ID};
use ping_pong_messages::{PingPongMsg, Resume};

/// Schedule a resume timer delivering `PingPongMsg::Resume` to self after
/// a duration derived from the current round number. Stores the `TimerId`
/// in `current_timer`.
pub fn schedule_resume<R: BloxRuntime>(
    self_id: ActorId,
    self_ref: &ActorRef<PingPongMsg, R>,
    timer_ref: &ActorRef<TimerCommand, R>,
    round: u32,
    current_timer: &mut Option<TimerId>,
) {
    let duration_ms = 2000 + (round as u64 * 500);
    let id = next_timer_id();
    let target = self_ref.clone();
    let deliver = alloc::boxed::Box::new(move || {
        let _ = target.try_send(TIMER_ACTOR_ID, PingPongMsg::Resume(Resume));
    });
    let _ = timer_ref.try_send(
        self_id,
        TimerCommand::Set {
            id,
            after_ms: duration_ms,
            deliver,
        },
    );
    *current_timer = Some(id);
}

/// Cancel a pending timer by its ID (if any) and clear the stored timer.
pub fn cancel_timer_by_id<R: BloxRuntime>(
    self_id: ActorId,
    timer_ref: &ActorRef<TimerCommand, R>,
    current_timer: &mut Option<TimerId>,
) {
    if let Some(id) = current_timer.take() {
        let _ = timer_ref.try_send(self_id, TimerCommand::Cancel { id });
    }
}
