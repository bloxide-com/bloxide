// Copyright 2025 Bloxide, all rights reserved

use super::{RootComponents, RootStates};
use crate::blox::root::states::*;
use bloxide::core::state_machine::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Uninit;

impl State<RootComponents> for Uninit {
    fn parent(&self) -> RootStates {
        RootStates::Uninit(Uninit)
    }

    fn handle_message(
        &self,
        _state_machine: &mut StateMachine<RootComponents>,
        _msg: <RootComponents as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents as Components>::MessageSet>> {
        None
    }
}
