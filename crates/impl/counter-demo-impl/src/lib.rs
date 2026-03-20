// Copyright 2025 Bloxide, all rights reserved
//! Concrete behavior implementation for the layered counter demo.
#![no_std]

pub mod prelude;

use counter_actions::CountsTicks;

/// Concrete behavior injected by the wiring binary.
#[derive(Debug, Default, Clone, Copy)]
pub struct CounterBehavior {
    pub count: u8,
}

impl CountsTicks for CounterBehavior {
    type Count = u8;

    fn count(&self) -> u8 {
        self.count
    }

    fn set_count(&mut self, count: u8) {
        self.count = count;
    }
}
