#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod prelude;

mod ctx;
mod events;
mod spec;

#[cfg(test)]
mod tests;

pub use ctx::PongCtx;
pub use events::PongEvent;
pub use spec::{PongSpec, PongState};
