// Copyright 2025 Bloxide, all rights reserved

use crate::{blox::supervisor::messaging::*, prelude::*};
use futures_core::stream::Stream;

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
    pub state_machine: StateMachine<C>,
    pub receivers: C::Receivers,
}

impl<C> Blox<C>
where
    C: Components,
    C::States: State<C> + Clone + PartialEq + Default,
    C::ExtendedState: ExtendedState,
{
    pub fn new(
        receivers: C::Receivers,
        extended_state: C::ExtendedState,
        self_handles: C::Handles,
    ) -> Self {
        Self {
            state_machine: StateMachine::<C>::new(extended_state, self_handles),
            receivers,
        }
    }
}

//Implement Runnable or RunnableLocal depending on if the blox implements Send
pub trait Runnable<B: Components> {
    fn run(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
}

pub trait RunnableLocal<B: Components> {
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

/// Core trait that each runtime must implement.
pub trait Runtime: Clone + Send + 'static {
    type MessageHandle<P: Send + 'static>: MessageSender<PayloadType = P> + Clone + Send + 'static;

    /// Spawn a future onto this runtime’s executor.
    fn spawn<F>(f: F)
    where
        F: Future<Output = ()> + Send + 'static;

    /// Convert a “ReceiverType” from the handle into a `Stream` of messages.
    type ReceiverStream<P: Send + 'static>: Stream<Item = Message<P>> + Unpin + Send + 'static;

    /// Convert the runtime’s “receiver” into a `Stream`.
    fn to_stream<P: Send + 'static>(
        receiver: <Self::MessageHandle<P> as MessageSender>::ReceiverType,
    ) -> Self::ReceiverStream<P>;
}
