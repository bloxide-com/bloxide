// Copyright 2025 Bloxide, all rights reserved
#![no_std]

extern crate alloc;

pub mod actions;
pub mod command;
pub mod queue;
pub mod service;

pub use actions::{cancel_timer, set_timer, HasTimerRef};
pub use command::{next_timer_id, TimerCommand, TimerId, TIMER_ACTOR_ID};
pub use queue::TimerQueue;
pub use service::TimerService;
