// Copyright 2025 Bloxide, all rights reserved
//! Worker actor blox — runtime-agnostic.
//!
//! States:
//! - `Waiting` (initial): accumulates peer introductions, awaits `DoWork`
//!
//! When `DoWork` is received, the transition actions process the work,
//! broadcast the result to peers, and notify the pool. The guard then
//! returns `Decision::Done`: the machine runs the same cleanup ritual as
//! `Stop` (exit chain + `on_init_entry`), and the run loop ends the task —
//! clean completion.
//!
//! The ctrl stream (`PeerCtrl<WorkerMsg, R>`) is polled at higher priority
//! than the domain stream (`WorkerMsg`) so all `AddPeer` messages are
//! processed before `DoWork` is dispatched.
#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod generated;
pub mod prelude;

#[cfg(all(test, feature = "std"))]
mod tests;

pub use generated::*;
