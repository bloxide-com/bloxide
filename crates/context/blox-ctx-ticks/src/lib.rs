// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for tick-counting behavior.
//!
//! Provides the `increment_count` action function (concrete params,
//! no accessor traits — invariant #14). Action functions return
//! `ActionResult` so guards can react to failures (uniform contract).
#![no_std]

use bloxide_core::transition::ActionResult;

/// Increment a count by one.
pub fn increment_count<Count: Copy + core::ops::Add<Output = Count> + From<u8>>(
    count: &mut Count,
) -> ActionResult {
    *count = *count + Count::from(1);
    ActionResult::Ok
}
