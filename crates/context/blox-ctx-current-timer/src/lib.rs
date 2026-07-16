// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for the current-timer-id behavior.
//!
//! Provides the `HasCurrentTimer` delegatable behavior trait.  The trait
//! definition lives here (with the data contract), not in the actions crate.
#![no_std]

use bloxide_macros::delegatable;
use bloxide_timer::TimerId;

/// Provides read/write access to the current pending timer ID.
#[delegatable]
pub trait HasCurrentTimer {
    fn current_timer(&self) -> Option<TimerId>;
    fn set_current_timer(&mut self, timer: Option<TimerId>);
}
