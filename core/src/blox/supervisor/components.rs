// Copyright 2025 Bloxide, all rights reserved

use super::{ext_state::*, messaging::*, states::*};
use crate::{
    merge::{MergedItem, MergedStream2},
    prelude::*,
};
use futures_util::StreamExt;
use log::*;
pub struct SupervisorComponents<R: Runtime> {
    phantom: PhantomData<R>,
}

pub struct SupervisorHandles<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    <R::MessageHandle<SupervisorPayload> as MessageSender>::ReceiverType: Send,
{
    pub standard_handle: R::MessageHandle<StandardPayload<R>>,
    pub supervisor_handle: R::MessageHandle<SupervisorPayload>,
}

pub struct SupervisorReceivers<R: Runtime>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    <R::MessageHandle<SupervisorPayload> as MessageSender>::ReceiverType: Send,
{
    pub standard_receiver: <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType,
    pub supervisor_receiver: <R::MessageHandle<SupervisorPayload> as MessageSender>::ReceiverType,
}

impl<R: Runtime> Components for SupervisorComponents<R>
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    <R::MessageHandle<SupervisorPayload> as MessageSender>::ReceiverType: Send,
{
    type States = SupervisorStateEnum;
    type MessageSet = SupervisorMessageSet<R>;
    type ExtendedState = SupervisorExtendedState<R>;
    type Receivers = SupervisorReceivers<R>;
    type Handles = SupervisorHandles<R>;
}

pub struct SupervisorInitArgs<R: Runtime>
where
    R::MessageHandle<StandardPayload<R>>:
        MessageSender<PayloadType = StandardPayload<R>> + Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
{
    pub root_standard_handle: R::MessageHandle<StandardPayload<R>>,
    pub root_future: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl<R: Runtime> Runnable<SupervisorComponents<R>> for Blox<SupervisorComponents<R>>
where
    R::MessageHandle<StandardPayload<R>>:
        MessageSender<PayloadType = StandardPayload<R>> + Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    R::MessageHandle<SupervisorPayload>:
        MessageSender<PayloadType = SupervisorPayload> + Clone + Send + 'static,
    <R::MessageHandle<SupervisorPayload> as MessageSender>::ReceiverType: Send,
{
    fn run(mut self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            self.state_machine.init(
                &SupervisorStateEnum::Uninit(Uninit),
                &SupervisorStateEnum::Running(Running),
            );

            let standard_stream = R::to_stream(self.receivers.standard_receiver);
            let supervisor_stream = R::to_stream(self.receivers.supervisor_receiver);

            let mut merged = MergedStream2::new(standard_stream, supervisor_stream);

            while let Some(item) = merged.next().await {
                match item {
                    MergedItem::From1(std_msg) => {
                        let msg = SupervisorMessageSet::StandardMessage(std_msg);
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(msg, &current_state);
                    }
                    MergedItem::From2(sup_msg) => {
                        let msg = SupervisorMessageSet::SupervisorMessage(sup_msg);
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(msg, &current_state);
                    }
                }
            }

            trace!("All channels closed. Supervisor run loop complete.");
        })
    }
}
