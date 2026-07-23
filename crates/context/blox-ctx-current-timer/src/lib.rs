// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for the current-timer-id behavior.
//!
//! Provides the `HasCurrentTimer` behavior trait.
//! definition lives here (with the data contract), not in the actions crate.
#![no_std]

extern crate alloc;

use bloxide_core::{capability::BloxRuntime, messaging::ActorRef, ActorId};
use bloxide_timer::command::{next_timer_id, TimerCommand, TimerId, TIMER_ACTOR_ID};
use ping_pong_messages::{PingPongMsg, Resume};


/// Provides read/write access to the current pending timer ID.
pub trait HasCurrentTimer {
    fn current_timer(&self) -> Option<TimerId>;
    fn set_current_timer(&mut self, timer: Option<TimerId>);
}

/// Schedule a resume timer delivering `PingPongMsg::Resume` to self after
/// `duration_ms` milliseconds. Returns the `TimerId` for the caller to store.
pub fn schedule_resume<R: BloxRuntime>(
    self_id: ActorId,
    self_ref: &ActorRef<PingPongMsg, R>,
    timer_ref: &ActorRef<TimerCommand, R>,
    duration_ms: u64,
) -> TimerId {
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
    id
}

/// Cancel a pending timer by its ID (if any).
pub fn cancel_timer_by_id<R: BloxRuntime>(
    self_id: ActorId,
    timer_ref: &ActorRef<TimerCommand, R>,
    timer_id: Option<TimerId>,
) {
    if let Some(id) = timer_id {
        let _ = timer_ref.try_send(self_id, TimerCommand::Cancel { id });
    }
}
