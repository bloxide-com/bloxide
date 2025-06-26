// Copyright 2025 Bloxide, all rights reserved

use super::{counting::Counting, idle::Idle, RootComponents, RootStates};
use crate::{
    blox::{
        demo_counter::{
            components::{CounterComponents, CounterHandles, CounterReceivers},
            ext_state::{CounterExtendedState, CounterInitArgs},
            messaging::{CountEvent, CounterPayload},
        },
        demo_root::messaging::RootMessageSet,
        supervisor::messaging::*,
    },
    prelude::*,
};
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Starting;

impl<R: Runtime> State<RootComponents<R>> for Starting
where
    R::MessageHandle<StandardPayload<R>>: Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    R::MessageHandle<CounterPayload>: Clone + Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send,
{
    fn parent(&self) -> RootStates {
        RootStates::Idle(Idle)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<RootComponents<R>>,
        msg: <RootComponents<R> as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents<R> as Components>::MessageSet>> {
        match msg {
            RootMessageSet::StandardMessage(msg) => match msg.payload {
                StandardPayload::StandardChannel(new_standard_handle, standard_receiver) => {
                    let (counter_handle, counter_receiver) =
                        R::MessageHandle::<CounterPayload>::create_channel_with_size(
                            new_standard_handle.id(),
                            32, // TODO: make this configurable
                        );

                    state_machine.extended_state.counter_handle = Some(counter_handle.clone());

                    let counter_receivers = CounterReceivers {
                        standard_receiver,
                        counter_receiver,
                    };

                    let counter_handles = CounterHandles {
                        standard_handle: new_standard_handle.clone(),
                        counter_handle: counter_handle.clone(),
                    };

                    let counter_init_args = CounterInitArgs {
                        supervisor_handle: state_machine.extended_state.supervisor_handle.clone(),
                    };
                    let mut counter_extended_state = CounterExtendedState::new(counter_init_args);
                    counter_extended_state
                        .subscribers
                        .push(state_machine.self_handles.counter_handle.clone());

                    let counter_blox = Blox::<CounterComponents<R>>::new(
                        counter_receivers,
                        counter_extended_state,
                        counter_handles,
                    );

                    let counter_future = Box::pin(async move {
                        Box::new(counter_blox).run().await;
                    });

                    let spawn_request = SupervisorPayload::Spawn(counter_future);

                    let supervisor_handle = state_machine.extended_state.supervisor_handle.clone();
                    if let Err(e) = supervisor_handle.try_send(Message::new(
                        state_machine.self_handles.standard_handle.id(),
                        spawn_request,
                    )) {
                        error!("Failed to send message: {:?}", e);
                    }

                    state_machine
                        .extended_state
                        .counter_handle
                        .as_ref()
                        .unwrap()
                        .try_send(Message::new(
                            state_machine.self_handles.standard_handle.id(),
                            CounterPayload::SetMax(Box::new(4)),
                        ))
                        .unwrap_or_else(|e| error!("Failed to send message: {:?}", e));

                    Some(Transition::To(RootStates::Counting(Counting)))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn on_entry(&self, state_machine: &mut StateMachine<RootComponents<R>>) {
        trace!("State on_entry: {:?}", self);
        // Request a new standard handle to start the counter blox
        let handle = state_machine.extended_state.supervisor_handle.clone();
        if let Err(e) = handle.try_send(Message::new(
            state_machine.self_handles.standard_handle.id(),
            SupervisorPayload::RequestNewStandardHandle(32), // TODO: make this configurable
        )) {
            error!("Failed to send message: {:?}", e);
        }
    }

    fn on_exit(&self, state_machine: &mut StateMachine<RootComponents<R>>) {
        trace!("State on_exit: {:?}", self);
        // Start the counter ping pong
        state_machine
            .extended_state
            .counter_handle
            .as_ref()
            .unwrap()
            .try_send(Message::new(
                state_machine.self_handles.standard_handle.id(),
                CounterPayload::CountEvent(Box::new(CountEvent::StartCounting)),
            ))
            .unwrap_or_else(|e| error!("Failed to send message: {:?}", e));
    }
}
