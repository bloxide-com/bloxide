// Copyright 2025 Bloxide, all rights reserved

use super::{RootComponents, RootStates};
use crate::blox::counter::{components::*, ext_state::*, messaging::*};
use crate::blox::root::{messaging::*, states::*};
use bloxide::{
    core::{messaging::*, state_machine::*},
    runtime::*,
    supervisor::*,
    std_exports::*,
};
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Starting;

impl State<RootComponents> for Starting {
    fn parent(&self) -> RootStates {
        RootStates::Idle(Idle)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<RootComponents>,
        msg: <RootComponents as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents as Components>::MessageSet>> {
        match msg {
            RootMessage::StandardMessage(msg) => match msg.payload {
                StandardPayload::StandardChannel(new_standard_handle, standard_receiver) => {
                    let (counter_handle, counter_receiver) = Handle::create_channel_with_size(
                        new_standard_handle.dest_id,
                        DEFAULT_CHANNEL_SIZE,
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

                    let mut counter_extended_state = CounterExtendedState::new(());
                    counter_extended_state
                        .subscribers
                        .push(state_machine.self_handles.counter_handle.clone());

                    let counter_blox = Blox::<CounterComponents>::new(
                        new_standard_handle.clone(),
                        counter_receivers,
                        counter_extended_state,
                        counter_handles,
                    );

                    let spawn_request = Box::new(counter_blox).into_request();

                    if let Err(e) = get_supervisor_handle().try_send(Message::new(
                        state_machine.self_handles.standard_handle.dest_id,
                        spawn_request,
                    )) {
                        error!("Root Failed to send message: {:?}", e);
                    }

                    state_machine
                        .extended_state
                        .counter_handle
                        .as_ref()
                        .unwrap()
                        .try_send(Message::new(
                            state_machine.self_handles.standard_handle.dest_id,
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

    fn on_entry(&self, state_machine: &mut StateMachine<RootComponents>) {
        trace!("State on_entry: {:?}", self);
        // Request a new standard handle to start the counter blox
        let _ = get_supervisor_handle().try_send(Message::new(
            state_machine.self_handles.standard_handle.dest_id,
            SupervisorPayload::RequestNewStandardHandle(DEFAULT_CHANNEL_SIZE),
        ));
    }

    fn on_exit(&self, state_machine: &mut StateMachine<RootComponents>) {
        trace!("State on_exit: {:?}", self);
        // Start the counter ping pong
        state_machine
            .extended_state
            .counter_handle
            .as_ref()
            .unwrap()
            .try_send(Message::new(
                state_machine.self_handles.standard_handle.dest_id,
                CounterPayload::CountEvent(Box::new(CountEvent::StartCounting)),
            ))
            .unwrap_or_else(|e| error!("Failed to send message: {:?}", e));
    }
}
