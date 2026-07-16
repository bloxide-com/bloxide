// Copyright 2025 Bloxide, all rights reserved
//! Action traits and generic functions for the layered counter demo.
//!
//! The `CountsTicks` trait now lives in the `blox-ctx-ticks` context crate.
//! Re-exported here for convenience. New code should import directly from
//! `blox_ctx_ticks`.
#![no_std]

pub use blox_ctx_ticks::CountsTicks;

pub mod prelude {
    pub use crate::{increment_count, CountsTicks};
}

/// Generic, reusable increment operation used by counter blox actions.
pub fn increment_count<C: CountsTicks>(ctx: &mut C) {
    let one = C::Count::from(1);
    ctx.set_count(ctx.count() + one);
}
