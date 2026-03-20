// Copyright 2025 Bloxide, all rights reserved
//! Pool actor blox — runtime-agnostic.
//!
//! States:
//! - `Idle` (initial): awaiting the first `SpawnWorker`
//! - `Active`: workers are running; accepts more `SpawnWorker` and `WorkDone`
//! - `AllDone` (terminal): all workers have reported completion
#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod ctx;
mod events;
pub mod prelude;
mod spec;

#[cfg(test)]
mod tests;

pub use ctx::PoolCtx;
pub use events::PoolEvent;
pub use spec::{PoolSpec, PoolState};
