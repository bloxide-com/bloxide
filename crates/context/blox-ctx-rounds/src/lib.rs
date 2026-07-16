// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for round-counting behavior.
//!
//! Provides the `CountsRounds` delegatable behavior trait.  The trait
//! definition lives here (with the data contract), not in the actions crate.
#![no_std]

use bloxide_macros::delegatable;

/// Tracks the current round number in the ping-pong exchange.
#[delegatable]
pub trait CountsRounds {
    type Round: Copy
        + PartialEq
        + PartialOrd
        + core::ops::Add<Output = Self::Round>
        + From<u8>
        + core::fmt::Display;
    fn round(&self) -> Self::Round;
    fn set_round(&mut self, round: Self::Round);
}
