// Copyright 2025 Bloxide, all rights reserved
#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod generated;
pub mod prelude;

#[cfg(all(test, feature = "std"))]
mod tests;

pub use generated::*;

/// Number of ticks after which the counter completes (`Decision::Done`).
pub const DONE_AT_COUNT: u32 = 2;
