// Copyright 2025 Bloxide, all rights reserved
#![no_std]

//! Messaging action functions for the bloxide framework.
//!
//! This crate provides action functions for sending ping/pong messages
//! between actors. The accessor traits (HasSelfRef, HasPeerRef) have been
//! removed — action functions now take concrete parameters directly.

use bloxide_core::{
    capability::BloxRuntime, messaging::ActorRef, transition::ActionResult, ActorId,
};
use ping_pong_messages::{Ping, PingPongMsg, Pong};

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
