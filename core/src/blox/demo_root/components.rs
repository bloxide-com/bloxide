// Copyright 2025 Bloxide, all rights reserved

use super::{ext_state::*, messaging::*, states::*};
use crate::{
    blox::demo_counter::messaging::CounterPayload,
    merge::{MergedItem, MergedStream2},
    prelude::*,
};
use futures_util::StreamExt;
use log::*;
pub struct RootComponents<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    phantom: PhantomData<R>,
}

impl<R: Runtime> Components for RootComponents<R>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    type States = RootStates;
    type MessageSet = RootMessageSet<R>;
    type ExtendedState = RootExtState<R>;
    type Receivers = RootReceivers<R>;
    type Handles = RootHandles<R>;
}

pub struct RootHandles<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    pub standard_handle: R::MessageHandle<StandardPayload<R>>,
    pub counter_handle: R::MessageHandle<CounterPayload>,
}

pub struct RootReceivers<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    pub std_rx: <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType,
    pub counter_rx: <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType,
}

impl<R: Runtime> Runnable<RootComponents<R>> for Blox<RootComponents<R>>
where
    R::MessageHandle<StandardPayload<R>>:
        MessageSender<PayloadType = StandardPayload<R>> + Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    R::MessageHandle<CounterPayload>:
        MessageSender<PayloadType = CounterPayload> + Clone + Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send,
{
    fn run(mut self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            self.state_machine
                .init(&RootStates::Uninit(Uninit), &RootStates::Starting(Starting));

            let standard_stream = R::to_stream(self.receivers.std_rx);
            let counter_stream = R::to_stream(self.receivers.counter_rx);

            let mut merged = MergedStream2::new(standard_stream, counter_stream);

            while let Some(item) = merged.next().await {
                match item {
                    MergedItem::From1(std_msg) => {
                        let msg = RootMessageSet::StandardMessage(std_msg);
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(msg, &current_state);
                    }
                    MergedItem::From2(sup_msg) => {
                        let msg = RootMessageSet::CounterMessage(sup_msg);
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(msg, &current_state);
                    }
                }
            }

            trace!("All channels closed. Supervisor run loop complete.");
        })
    }
}
