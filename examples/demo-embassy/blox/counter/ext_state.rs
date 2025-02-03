// Copyright 2025 Bloxide, all rights reserved

use super::runtime::*;
use bloxide::core::state_machine::*;
use bloxide::std_exports::*;
#[derive(Default)]
pub struct CounterExtendedState {
    pub count: usize,
    pub max: usize,
    pub min: usize,
    pub subscribers: Vec<CounterHandle>,
}
impl ExtendedState for CounterExtendedState {
    type InitArgs = ();
    fn new(_args: Self::InitArgs) -> Self {
        Self::default()
    }
}
