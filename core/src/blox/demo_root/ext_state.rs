// Copyright 2025 Bloxide, all rights reserved

use crate::{
    blox::{demo_counter::messaging::CounterPayload, supervisor::SupervisorPayload},
    prelude::*,
};

#[derive(Debug)]
pub struct RootExtState<R: Runtime> {
    pub supervisor_handle: R::MessageHandle<SupervisorPayload>,
    pub counter_handle: Option<R::MessageHandle<CounterPayload>>,
}

pub struct RootInitArgs<R: Runtime> {
    pub supervisor_handle: R::MessageHandle<SupervisorPayload>,
    pub counter_handle: Option<R::MessageHandle<CounterPayload>>,
}

impl<R: Runtime> ExtendedState for RootExtState<R> {
    type InitArgs = RootInitArgs<R>;
    fn new(args: Self::InitArgs) -> Self {
        Self {
            supervisor_handle: args.supervisor_handle,
            counter_handle: args.counter_handle,
        }
    }
}
