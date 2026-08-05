// Copyright 2025 Bloxide, all rights reserved
//! Pong actor blox — runtime-agnostic.
#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod generated;
pub mod prelude;

#[cfg(all(test, feature = "std"))]
mod tests;

pub use generated::{PongCtx, PongEvent, PongSpec, PongState};
