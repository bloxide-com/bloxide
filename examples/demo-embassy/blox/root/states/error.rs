// Copyright 2025 Bloxide, all rights reserved

use super::{RootComponents, RootStates};
use crate::blox::root::states::*;
use bloxide::core::state_machine::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Error;

impl State<RootComponents> for Error {
    fn parent(&self) -> RootStates {
        RootStates::Idle(Idle)
    }

    fn handle_message(
        &self,
        _state_machine: &mut StateMachine<RootComponents>,
        _msg: <RootComponents as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents as Components>::MessageSet>> {
        None
    }
}
