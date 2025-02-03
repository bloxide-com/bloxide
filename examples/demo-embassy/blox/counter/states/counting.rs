// Copyright 2025 Bloxide, all rights reserved

use crate::blox::counter::{components::*, ext_state::*, messaging::*, states::*};
use bloxide::core::messaging::*;
use bloxide::core::state_machine::*;
use bloxide::std_exports::*;
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Counting;

impl State<CounterComponents> for Counting {
    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Uninit(Uninit)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<CounterComponents>,
        message: CounterMessageSet,
    ) -> Option<Transition<CounterStateEnum, CounterMessageSet>> {
        trace!("[Counting] handle_message: {:?}", message);

        match message {
            CounterMessageSet::CounterMessage(msg) => match &msg.payload {
                CounterPayload::Increment(amount) => {
                    self.do_increment(**amount, &mut state_machine.extended_state);
                    if state_machine.extended_state.count >= state_machine.extended_state.max {
                        for subscriber in state_machine.extended_state.subscribers.iter() {
                            let _ = subscriber.try_send(Message::new(
                                state_machine.self_handles.standard_handle.dest_id,
                                CounterPayload::CountEvent(Box::new(CountEvent::MaxReached)),
                            ));
                        }
                        Some(Transition::To(CounterStateEnum::Finished(Finished)))
                    } else {
                        None
                    }
                }
                CounterPayload::Decrement(amount) => {
                    self.do_decrement(**amount, &mut state_machine.extended_state);
                    if state_machine.extended_state.count <= state_machine.extended_state.min {
                        for subscriber in state_machine.extended_state.subscribers.iter() {
                            let _ = subscriber.try_send(Message::new(
                                state_machine.self_handles.standard_handle.dest_id,
                                CounterPayload::CountEvent(Box::new(CountEvent::MinReached)),
                            ));
                        }
                        Some(Transition::To(CounterStateEnum::Finished(Finished)))
                    } else {
                        None
                    }
                }
                CounterPayload::CountEvent(event) => match **event {
                    CountEvent::GetCount => {
                        debug!(
                            "[Counting] Current count: {} Max: {}",
                            state_machine.extended_state.count, state_machine.extended_state.max
                        );
                        for subscriber in state_machine.extended_state.subscribers.iter() {
                            let _ = subscriber.try_send(Message::new(
                                state_machine.self_handles.standard_handle.dest_id,
                                CounterPayload::SetCount(Box::new(
                                    state_machine.extended_state.count,
                                )),
                            ));
                        }
                        None
                    }
                    CountEvent::Reset => {
                        Some(Transition::To(CounterStateEnum::NotStarted(NotStarted)))
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }
}

impl Counting {
    fn do_increment(&self, amount: usize, data: &mut CounterExtendedState) {
        data.count += amount;
        debug!("[Counting] Incremented by {} to {}", amount, data.count);
    }

    fn do_decrement(&self, amount: usize, data: &mut CounterExtendedState) {
        data.count -= amount;
        debug!("[Counting] Decremented by {} to {}", amount, data.count);
    }
}
