// Copyright 2025 Bloxide, all rights reserved
//! Concrete behavior trait implementations shared by ping-pong demos.
//!
//! This crate provides a composite behavior type that implements all behavior
//! traits defined in the action crate. The wiring binary injects it into the
//! blox context at construction time.
//!
//! A different binary (e.g. targeting real hardware) could provide its own
//! impl crate with hardware-backed implementations of the same traits,
//! while reusing the same blox crate unchanged.

#![no_std]

pub mod prelude;

use bloxide_timer::TimerId;
use ping_pong_actions::{CountsRounds, HasCurrentTimer};

/// Composite behavior type for the Ping actor.
///
/// Implements both behavior traits. The binary injects this into
/// `PingCtx<R, PingBehavior>` at wiring time.
#[derive(Debug, Default, Clone)]
pub struct PingBehavior {
    pub round: u32,
    pub current_timer: Option<TimerId>,
}

impl CountsRounds for PingBehavior {
    type Round = u32;
    fn round(&self) -> u32 {
        self.round
    }
    fn set_round(&mut self, round: u32) {
        self.round = round;
    }
}

impl HasCurrentTimer for PingBehavior {
    fn current_timer(&self) -> Option<TimerId> {
        self.current_timer
    }
    fn set_current_timer(&mut self, timer: Option<TimerId>) {
        self.current_timer = timer;
    }
}
