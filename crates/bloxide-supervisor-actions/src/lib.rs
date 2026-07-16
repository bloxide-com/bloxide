// Copyright 2025 Bloxide, all rights reserved
#![no_std]
extern crate alloc;

pub mod lifecycle;
#[cfg(feature = "dynamic")]
pub mod spawn;

pub use lifecycle::{
    handle_done_or_failed, handle_health_check, handle_reset, record_alive, record_started,
    record_stopped, register_child, start_children, stop_all_children,
};
#[cfg(feature = "dynamic")]
pub use spawn::handle_spawn_request;
