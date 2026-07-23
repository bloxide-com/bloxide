// Copyright 2025 Bloxide, all rights reserved
#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod actions;
pub mod generated;
pub mod prelude;

#[cfg(all(test, feature = "std"))]
mod tests;

pub use generated::*;
