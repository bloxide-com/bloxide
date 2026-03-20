// Copyright 2025 Bloxide, all rights reserved
#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod prelude;

mod ctx;
mod events;
mod spec;

#[cfg(all(test, feature = "std"))]
mod tests;

pub use ctx::CounterCtx;
pub use events::CounterEvent;
pub use spec::{CounterSpec, CounterState};
