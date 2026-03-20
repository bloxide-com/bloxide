// Copyright 2025 Bloxide, all rights reserved
#![no_std]

pub mod prelude;

use bloxide_macros::blox_messages;

// Shared domain message type used by both the Ping and Pong bloxes.
// Using a single message enum demonstrates the common pattern where two actors
// communicate over a shared protocol — both actors use `ActorRef<PingPongMsg, R>`.
// Each blox only handles the variants it cares about; unmatched variants bubble
// or are silently dropped.
blox_messages! {
    pub enum PingPongMsg {
        Ping { round: u32 },
        Pong { round: u32 },
        Resume {},
    }
}
