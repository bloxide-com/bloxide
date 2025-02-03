// Copyright 2025 Bloxide, all rights reserved

use super::{RootComponents, RootStates};
use crate::blox::counter::messaging::*;
use crate::blox::root::{messaging::*, states::*};
use bloxide::core::{messaging::*, state_machine::*};
use bloxide::std_exports::*;

use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Counting;

impl State<RootComponents> for Counting {
    fn parent(&self) -> RootStates {
        RootStates::Uninit(Uninit)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<RootComponents>,
        msg: <RootComponents as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents as Components>::MessageSet>> {
        match msg {
            RootMessage::CounterMessage(msg) => match msg.payload {
                CounterPayload::CountEvent(event) => match *event {
                    CountEvent::MaxReached => Some(Transition::To(RootStates::Finished(Finished))),
                    CountEvent::MinReached => Some(Transition::To(RootStates::Finished(Finished))),
                    _ => None,
                },
                CounterPayload::SetCount(count) => {
                    info!("Current count: {}", count);
                    let _ = state_machine
                        .extended_state
                        .counter_handle
                        .as_ref()
                        .unwrap()
                        .try_send(Message::new(
                            state_machine.self_handles.standard_handle.dest_id,
                            CounterPayload::Increment(Box::new(1)),
                        ));
                    let _ = state_machine
                        .extended_state
                        .counter_handle
                        .as_ref()
                        .unwrap()
                        .try_send(Message::new(
                            state_machine.self_handles.standard_handle.dest_id,
                            CounterPayload::CountEvent(Box::new(CountEvent::GetCount)),
                        ));
                    None
                }
                _ => None,
            },
            _ => None,
        }
    }
}
