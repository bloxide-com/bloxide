// Copyright 2025 Bloxide, all rights reserved

use super::{RootComponents, RootStates};
use crate::blox::root::states::*;
use bloxide::core::state_machine::*;
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Finished;

impl State<RootComponents> for Finished {
    fn parent(&self) -> RootStates {
        RootStates::Idle(Idle)
    }

    fn handle_message(
        &self,
        _state_machine: &mut StateMachine<RootComponents>,
        _msg: <RootComponents as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents as Components>::MessageSet>> {
        //Program is finished, no more messages
        None
    }

    fn on_entry(&self, _data: &mut StateMachine<RootComponents>) {
        trace!("State on_entry: {:?}", self);
        info!("♫ I CAN ONLY COUNT TO 4 ♫");
    }
}
