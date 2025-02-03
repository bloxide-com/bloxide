// Copyright 2025 Bloxide, all rights reserved

use crate::blox::counter::{components::*, messaging::*, states::*};
use bloxide::core::state_machine::*;
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Idle;

impl State<CounterComponents> for Idle {
    fn parent(&self) -> CounterStateEnum {
        CounterStateEnum::Uninit(Uninit)
    }

    fn handle_message(
        &self,
        _state_machine: &mut StateMachine<CounterComponents>,
        message: CounterMessageSet,
    ) -> Option<Transition<CounterStateEnum, CounterMessageSet>> {
        trace!("State: {:?} handle_message: {:?}", self, message);
        None
    }
}
