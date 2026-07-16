// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for tick-counting behavior.
//!
//! Provides the `CountsTicks` delegatable behavior trait.  The trait
//! definition lives here (with the data contract), not in the actions crate.
#![no_std]

use bloxide_macros::delegatable;

/// Behavior trait for contexts that track a count.
#[delegatable]
pub trait CountsTicks {
    type Count: Copy + PartialOrd + core::ops::Add<Output = Self::Count> + From<u8>;
    fn count(&self) -> Self::Count;
    fn set_count(&mut self, count: Self::Count);
}
