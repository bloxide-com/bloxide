// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for tick-counting behavior.
//!
//! Provides the `increment_count` action function (concrete params,
//! no accessor traits — invariant #14). Action functions return
//! `ActionResult` so guards can react to failures (uniform contract).
#![no_std]

use bloxide_core::transition::ActionResult;

/// Increment the tick count by one.
pub fn increment_count(count: &mut u32) -> ActionResult {
    *count += 1;
    ActionResult::Ok
}
