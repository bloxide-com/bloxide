// Copyright 2025 Bloxide, all rights reserved

use super::*;
use crate::blox::counter::{components::*, messaging::*};
use bloxide::core::{messaging::*, state_machine::*};
use bloxide::std_exports::*;
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct NotStarted;

impl State<CounterComponents> for NotStarted {
    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Idle(Idle)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<CounterComponents>,
        message: CounterMessageSet,
    ) -> Option<Transition<CounterStateEnum, CounterMessageSet>> {
        trace!("[Idle] handle_message: {:?}", message);
        match message {
            CounterMessageSet::CounterMessage(msg) => self.handle_counter_msg(msg, state_machine),
            _ => None,
        }
    }
}

impl NotStarted {
    fn handle_counter_msg(
        &self,
        msg: Message<CounterPayload>,
        state_machine: &mut StateMachine<CounterComponents>,
    ) -> Option<Transition<CounterStateEnum, CounterMessageSet>> {
        match &msg.payload {
            CounterPayload::SetCount(new_value) => {
                state_machine.extended_state.count = **new_value;
                debug!(
                    "State: {:?} Set count to {}",
                    self, state_machine.extended_state.count
                );
                None
            }
            CounterPayload::SetMax(new_max) => {
                state_machine.extended_state.max = **new_max;
                debug!(
                    "State: {:?} New max set to {}",
                    self, state_machine.extended_state.max
                );
                None
            }
            CounterPayload::SetMin(new_min) => {
                state_machine.extended_state.min = **new_min;
                debug!(
                    "State: {:?} New min set to {}",
                    self, state_machine.extended_state.min
                );
                None
            }
            CounterPayload::CountEvent(event) => {
                trace!("State: {:?} Received CountEvent: {:?}", self, event);
                match **event {
                    CountEvent::GetCount => {
                        debug!(
                            "State: {:?} Current count: {}",
                            self, state_machine.extended_state.count
                        );

                        None
                    }
                    CountEvent::StartCounting => {
                        state_machine
                            .extended_state
                            .subscribers
                            .iter()
                            .for_each(|subscriber| {
                                let _ = subscriber.try_send(Message::new(
                                    state_machine.self_handles.standard_handle.dest_id,
                                    CounterPayload::SetCount(Box::new(
                                        state_machine.extended_state.count,
                                    )),
                                ));
                            });
                        Some(Transition::To(CounterStateEnum::Counting(Counting)))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
