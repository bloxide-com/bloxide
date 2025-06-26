// Copyright 2025 Bloxide, all rights reserved

use super::*;
use crate::{
    blox::demo_counter::{components::*, messaging::*},
    prelude::*,
};
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct NotStarted;

impl<R: Runtime> State<CounterComponents<R>> for NotStarted
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Idle(Idle)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<CounterComponents<R>>,
        message: CounterMessageSet<R>,
    ) -> Option<Transition<CounterStateEnum, CounterMessageSet<R>>> {
        match message {
            CounterMessageSet::CounterMessage(msg) => match &msg.payload {
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
                            state_machine.extended_state.subscribers.iter().for_each(
                                |subscriber| {
                                    let _ = subscriber.try_send(Message::new(
                                        state_machine.self_handles.standard_handle.id(),
                                        CounterPayload::SetCount(Box::new(
                                            state_machine.extended_state.count,
                                        )),
                                    ));
                                },
                            );
                            Some(Transition::To(CounterStateEnum::Counting(Counting)))
                        }
                        _ => None,
                    }
                }
                _ => None,
            },
            _ => None,
        }
    }
}
