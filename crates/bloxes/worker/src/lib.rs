// Copyright 2025 Bloxide, all rights reserved
//! Worker actor blox — runtime-agnostic.
//!
//! States:
//! - `Waiting` (initial): accumulates peer introductions, awaits `DoWork`
//! - `Done` (terminal): broadcasts result to peers, notifies pool
//!
//! The ctrl stream (`PeerCtrl<WorkerMsg, R>`) is polled at higher priority
//! than the domain stream (`WorkerMsg`) so all `AddPeer` messages are
//! processed before `DoWork` is dispatched.
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

pub use ctx::WorkerCtx;
pub use events::WorkerEvent;
pub use spec::{WorkerSpec, WorkerState};
