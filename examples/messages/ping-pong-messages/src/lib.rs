// Copyright 2025 Bloxide, all rights reserved
#![no_std]

pub mod prelude;

/// Round-number payload for a Ping message.
#[derive(Debug, Clone, Copy)]
pub struct Ping {
    pub round: u32,
}

/// Round-number payload for a Pong message.
#[derive(Debug, Clone, Copy)]
pub struct Pong {
    pub round: u32,
}

/// Sent by the timer to resume the ping-pong exchange after a pause.
#[derive(Debug, Clone, Copy)]
pub struct Resume;

/// Shared domain message type used by both the Ping and Pong bloxes.
///
/// Using a single message enum demonstrates the common pattern where two actors
/// communicate over a shared protocol — both actors use `ActorRef<PingPongMsg, R>`.
/// Each blox only handles the variants it cares about; unmatched variants bubble
/// or are silently dropped.
#[derive(Debug, Clone)]
pub enum PingPongMsg {
    Ping(Ping),
    Pong(Pong),
    /// Only handled by Ping; Pong never handles this variant.
    Resume(Resume),
}
