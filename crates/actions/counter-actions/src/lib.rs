// Copyright 2025 Bloxide, all rights reserved
//! Action traits and generic functions for the layered counter demo.
#![no_std]

use bloxide_macros::delegatable;

pub mod prelude {
    pub use crate::{increment_count, CountsTicks};
}

/// Behavior trait for contexts that track a count.
#[delegatable]
pub trait CountsTicks {
    type Count: Copy + PartialOrd + core::ops::Add<Output = Self::Count> + From<u8>;
    fn count(&self) -> Self::Count;
    fn set_count(&mut self, count: Self::Count);
}

/// Generic, reusable increment operation used by counter blox actions.
pub fn increment_count<C: CountsTicks>(ctx: &mut C) {
    let one = C::Count::from(1);
    ctx.set_count(ctx.count() + one);
}
