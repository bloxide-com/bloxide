#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod prelude;

mod ctx;
mod events;
mod spec;

#[cfg(test)]
mod tests;

pub use ctx::PingCtx;
pub use events::PingEvent;
pub use spec::{PingSpec, PingState};

pub const MAX_ROUNDS: u8 = 5;

/// After receiving `Pong(PAUSE_AT_ROUND)`, Active internally transitions to
/// Paused instead of self-transitioning. `Paused::on_entry` then sets a timer
/// to deliver `PingPongMsg::Resume` after `PAUSE_DURATION_MS` milliseconds.
pub const PAUSE_AT_ROUND: u8 = 2;
pub const PAUSE_DURATION_MS: u64 = 150;
