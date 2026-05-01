// Copyright 2025 Bloxide, all rights reserved
#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod generated;
pub mod actions;
pub mod prelude;

#[cfg(all(test, feature = "std"))]
mod tests;

pub use generated::*;

pub const MAX_ROUNDS: u8 = 5;

/// After receiving `Pong(PAUSE_AT_ROUND)`, Active internally transitions to
/// Paused instead of self-transitioning. `Paused::on_entry` then sets a timer
/// to deliver `PingPongMsg::Resume` after `PAUSE_DURATION_MS` milliseconds.
pub const PAUSE_AT_ROUND: u8 = 2;
pub const PAUSE_DURATION_MS: u64 = 150;
