// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for round-counting behavior.
//!
//! Provides the `increment_round` action function.
//! The `round` field is now a plain struct field on the context.
#![no_std]

use bloxide_core::transition::ActionResult;

/// Increment the round counter by 1.
pub fn increment_round(round: &mut u32) -> ActionResult {
    *round += 1;
    ActionResult::Ok
}
