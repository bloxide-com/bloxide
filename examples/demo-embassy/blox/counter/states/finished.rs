// Copyright 2025 Bloxide, all rights reserved

use crate::blox::counter::{components::*, messaging::*, states::*};
use bloxide::core::state_machine::*;
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Finished;

impl State<CounterComponents> for Finished {
    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Idle(Idle)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<CounterComponents>,
        message: CounterMessageSet,
    ) -> Option<Transition<CounterStateEnum, CounterMessageSet>> {
        trace!("[Finished] handle_message: {:?}", message);
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

    fn on_entry(&self, data: &mut StateMachine<CounterComponents>) {
        trace!("State on_entry: {:?}", self);
        info!("Finished!");
        info!("Count is {}", data.extended_state.count);
    }

    fn on_exit(&self, _data: &mut StateMachine<CounterComponents>) {
        trace!("State on_exit: {:?}", self);
    }
}
