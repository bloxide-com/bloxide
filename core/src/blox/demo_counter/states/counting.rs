// Copyright 2025 Bloxide, all rights reserved

use crate::{
    blox::demo_counter::{
        components::CounterComponents,
        messaging::{CountEvent, CounterMessageSet, CounterPayload},
    },
    prelude::*,
};
use log::*;

use super::{CounterStateEnum, Finished, NotStarted, Uninit};

#[derive(Clone, PartialEq, Debug)]
pub struct Counting;

impl<R: Runtime> State<CounterComponents<R>> for Counting
where
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send + 'static,
{
    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Uninit(Uninit)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<CounterComponents<R>>,
        message: CounterMessageSet<R>,
    ) -> Option<Transition<CounterStateEnum, CounterMessageSet<R>>> {
        match message {
            CounterMessageSet::CounterMessage(msg) => match &msg.payload {
                CounterPayload::Increment(amount) => {
                    state_machine.extended_state.count += **amount;
                    if state_machine.extended_state.count >= state_machine.extended_state.max {
                        for subscriber in state_machine.extended_state.subscribers.iter() {
                            let _ = subscriber.try_send(Message::new(
                                state_machine.self_handles.standard_handle.id(),
                                CounterPayload::CountEvent(Box::new(CountEvent::MaxReached)),
                            ));
                        }
                        Some(Transition::To(CounterStateEnum::Finished(Finished)))
                    } else {
                        None
                    }
                }
                CounterPayload::Decrement(amount) => {
                    state_machine.extended_state.count -= **amount;
                    if state_machine.extended_state.count <= state_machine.extended_state.min {
                        for subscriber in state_machine.extended_state.subscribers.iter() {
                            let _ = subscriber.try_send(Message::new(
                                state_machine.self_handles.standard_handle.id(),
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
                                state_machine.self_handles.standard_handle.id(),
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
