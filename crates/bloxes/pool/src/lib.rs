// Copyright 2025 Bloxide, all rights reserved
//! Pool actor blox — runtime-agnostic.
//!
//! States:
//! - `Idle` (initial): awaiting the first `SpawnWorker`
//! - `Spawning`: sent a spawn request to the supervisor, awaiting `SpawnedWorker` reply
//! - `Active`: workers are running; accepts more `SpawnWorker` and `WorkDone`
//!
//! When all workers have reported completion (`pending == 0`), the transition
//! guard returns `Guard::Stop`, returning the machine to `Init`.
#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod actions;
pub mod generated;
pub mod prelude;

#[cfg(test)]
mod tests;

pub use generated::{PoolCtx, PoolEvent, PoolSpec, PoolState};
