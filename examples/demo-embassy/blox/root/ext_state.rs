// Copyright 2025 Bloxide, all rights reserved

use crate::blox::counter::runtime::*;
use bloxide::core::state_machine::*;

#[derive(Debug)]
pub struct RootExtState {
    pub counter_handle: Option<CounterHandle>,
}

pub struct RootInitArgs {
    pub counter_handle: Option<CounterHandle>,
}

impl ExtendedState for RootExtState {
    type InitArgs = RootInitArgs;
    fn new(args: Self::InitArgs) -> Self {
        Self {
            counter_handle: args.counter_handle,
        }
    }
}
