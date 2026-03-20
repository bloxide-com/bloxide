// Copyright 2025 Bloxide, all rights reserved
//! Prelude for the `bloxide-timer` crate.
//!
//! Import with `use bloxide_timer::prelude::*;` for quick access to commonly used types.

pub use crate::actions::{cancel_timer, set_timer, HasTimerRef};
pub use crate::command::{next_timer_id, TimerCommand, TimerId, TIMER_ACTOR_ID};
pub use crate::queue::TimerQueue;
pub use crate::service::TimerService;
