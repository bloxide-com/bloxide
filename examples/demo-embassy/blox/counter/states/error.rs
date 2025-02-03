// Copyright 2025 Bloxide, all rights reserved

use crate::blox::counter::{components::*, messaging::*, states::*};
use bloxide::core::state_machine::*;
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Error;

impl State<CounterComponents> for Error {
    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Idle(Idle)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<CounterComponents>,
        message: CounterMessageSet,
    ) -> Option<Transition<CounterStateEnum, CounterMessageSet>> {
        trace!("[Error] handle_message: {:?}", message);
        match message {
            CounterMessageSet::CounterMessage(msg) => match &msg.payload {
                CounterPayload::CountEvent(event) => match **event {
                    CountEvent::Reset => {
                        state_machine.extended_state.count = 0;
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
