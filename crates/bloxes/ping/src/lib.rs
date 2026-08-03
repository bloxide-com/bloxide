// Copyright 2025 Bloxide, all rights reserved
#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod generated;
pub mod prelude;

#[cfg(all(test, feature = "std"))]
mod tests;

pub use generated::*;

pub const MAX_ROUNDS: u8 = 5;

/// After receiving `Pong(PAUSE_AT_ROUND)`, Active transitions to Paused.
/// `Paused::on_entry` schedules a resume timer whose duration is derived
/// from the round number (see `schedule_resume`: 2000 + round × 500 ms).
pub const PAUSE_AT_ROUND: u8 = 2;
