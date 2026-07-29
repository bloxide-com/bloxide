// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for the ping/pong demo.
//!
//! Action functions for peer/self messaging (`send_ping`, `send_pong`,
//! `send_initial_ping`) and the pause/resume timer (`schedule_resume`).
//! Timer primitives come from the `bloxide-timer` platform feature crate —
//! this crate holds only ping/pong-domain logic (spec 20: Platform Feature
//! Pattern, feature crates vs domain context crates).
#![no_std]

extern crate alloc;

use bloxide_core::{
    capability::BloxRuntime, messaging::ActorRef, transition::ActionResult, ActorId,
};
use bloxide_timer::command::{TimerCommand, TimerId};
use ping_pong_messages::{Ping, PingPongMsg, Pong, Resume};

/// Send a `PingPongMsg::Ping` to the peer with the given round number.
pub fn send_ping<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    round: u32,
) -> ActionResult {
    ActionResult::from(peer_ref.try_send(self_id, PingPongMsg::Ping(Ping { round })))
}

/// Send a `PingPongMsg::Pong` to the peer echoing the round from the received Ping.
pub fn send_pong<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    ping: &Ping,
) -> ActionResult {
    ActionResult::from(peer_ref.try_send(self_id, PingPongMsg::Pong(Pong { round: ping.round })))
}

/// Send a `PingPongMsg::Ping` to the peer only if this is the first round (round == 1).
/// This is an entry action — no return value.
pub fn send_initial_ping<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    round: u32,
) {
    if round == 1 {
        let _ = peer_ref.try_send(self_id, PingPongMsg::Ping(Ping { round }));
    }
}

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
    let id = bloxide_timer::actions::set_timer(
        self_id,
        timer_ref,
        duration_ms,
        self_ref,
        PingPongMsg::Resume(Resume),
    );
    *current_timer = Some(id);
}
