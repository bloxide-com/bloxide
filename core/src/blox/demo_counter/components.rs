// Copyright 2025 Bloxide, all rights reserved

use super::{ext_state::*, messaging::*, states::*};
use crate::{
    merge::{MergedItem, MergedStream2},
    prelude::*,
};
use futures_util::stream::StreamExt;
use log::*;
pub struct CounterComponents<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    phantom: PhantomData<R>,
}

impl<R: Runtime> Components for CounterComponents<R>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    type States = CounterStateEnum;
    type MessageSet = CounterMessageSet<R>;
    type ExtendedState = CounterExtendedState<R>;
    type Receivers = CounterReceivers<R>;
    type Handles = CounterHandles<R>;
}

pub struct CounterHandles<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    pub standard_handle: R::MessageHandle<StandardPayload<R>>,
    pub counter_handle: R::MessageHandle<CounterPayload>,
}

pub struct CounterReceivers<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    pub standard_receiver: <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType,
    pub counter_receiver: <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType,
}

impl<R: Runtime> Runnable<CounterComponents<R>> for Blox<CounterComponents<R>>
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
            self.state_machine.init(
                &CounterStateEnum::Uninit(Uninit),
                &CounterStateEnum::NotStarted(NotStarted),
            );

            let standard_stream = R::to_stream(self.receivers.standard_receiver);
            let counter_stream = R::to_stream(self.receivers.counter_receiver);

            let mut merged = MergedStream2::new(standard_stream, counter_stream);

            while let Some(item) = merged.next().await {
                match item {
                    MergedItem::From1(std_msg) => {
                        let msg = CounterMessageSet::StandardMessage(std_msg);
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(msg, &current_state);
                    }
                    MergedItem::From2(sup_msg) => {
                        let msg = CounterMessageSet::CounterMessage(sup_msg);
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(msg, &current_state);
                    }
                }
            }

            trace!("All channels closed. Supervisor run loop complete.");
        })
    }
}
