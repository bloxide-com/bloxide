// Copyright 2025 Bloxide, all rights reserved
#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod actions;
pub mod command;
pub mod prelude;
pub mod queue;
pub mod service;
#[cfg(feature = "std")]
pub mod test_utils;

pub use actions::{cancel_timer, set_timer, HasTimerRef};
pub use command::{next_timer_id, TimerCommand, TimerId, TIMER_ACTOR_ID};
pub use queue::TimerQueue;
pub use service::TimerService;
#[cfg(feature = "std")]
pub use test_utils::VirtualClock;
