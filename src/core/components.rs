// Copyright 2025 Bloxide, all rights reserved

use crate::{
    core::{messaging::*, state_machine::*},
    runtime::*,
    std_exports::*,
    supervisor::messaging::*,
};

// A trait to encapsulate types needed for a blox
pub trait Components {
    type ExtendedState: ExtendedState;
    type States: StateEnum + Default;
    type MessageSet: MessageSet;
    type Receivers;
    type Handles;
}

//The main blox struct.  Bloxes are differentiated by their components
//Anything that all Bloxes should have is stored here
pub struct Blox<C: Components> {
    pub handle: StandardMessageHandle,
    pub state_machine: StateMachine<C>,
    pub receivers: C::Receivers,
}

impl<C> Blox<C>
where
    C: Components,
    C::States: State<C> + Clone + PartialEq + Default,
    C::ExtendedState: ExtendedState,
    StandardMessageHandle: MessageSender,
{
    pub fn new(
        standard_handle: StandardMessageHandle,
        receivers: C::Receivers,
        extended_state: C::ExtendedState,
        self_handles: C::Handles,
    ) -> Self {
        Self {
            handle: standard_handle.clone(),
            state_machine: StateMachine::<C>::new(extended_state, self_handles),
            receivers,
        }
    }
}

//Implement Runnable or RunnableLocal depending on if the blox implements Send
pub trait Runnable<C: Components> {
    fn run(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
    fn into_request(self: Box<Self>) -> SupervisorPayload
    where
        Self: Send + 'static,
    {
        let closure = move || {
            Box::pin(async move { self.run().await })
                as Pin<Box<dyn Future<Output = ()> + Send + 'static>>
        };

        SupervisorPayload::Spawn(Box::new(closure))
    }
}

pub trait RunnableLocal<C: Components> {
    fn run_local(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + 'static>>;
    fn into_request(self: Box<Self>) -> SupervisorLocalPayload
    where
        Self: Send + 'static,
    {
        let closure = move || {
            Box::pin(async move { self.run_local().await })
                as Pin<Box<dyn Future<Output = ()> + 'static>>
        };

        SupervisorLocalPayload::SpawnLocal(Box::new(closure))
    }
}
