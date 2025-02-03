// Copyright 2025 Bloxide, all rights reserved

mod counting;
mod error;
mod finished;
mod idle;
pub(super) mod starting;
mod uninit;

use super::components::RootComponents;
use bloxide::core::{components::*, state_machine::*};
use counting::Counting;
use error::Error;
use finished::Finished;
use idle::Idle;
use starting::Starting;
use uninit::Uninit;

#[derive(Clone, PartialEq, Debug)]
pub enum RootStates {
    Uninit(Uninit),
    Idle(Idle),
    Starting(Starting),
    Counting(Counting),
    Finished(Finished),
    Error(Error),
}

impl State<RootComponents> for RootStates {
    fn handle_message(
        &self,
        state_machine: &mut StateMachine<RootComponents>,
        message: <RootComponents as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents as Components>::MessageSet>> {
        match self {
            RootStates::Uninit(uninit) => uninit.handle_message(state_machine, message),
            RootStates::Counting(counting) => counting.handle_message(state_machine, message),
            RootStates::Finished(finished) => finished.handle_message(state_machine, message),
            RootStates::Error(error) => error.handle_message(state_machine, message),
            RootStates::Idle(idle) => idle.handle_message(state_machine, message),
            RootStates::Starting(starting) => starting.handle_message(state_machine, message),
        }
    }

    fn on_entry(&self, state_machine: &mut StateMachine<RootComponents>) {
        match self {
            RootStates::Uninit(s) => s.on_entry(state_machine),
            RootStates::Counting(s) => s.on_entry(state_machine),
            RootStates::Finished(s) => s.on_entry(state_machine),
            RootStates::Error(s) => s.on_entry(state_machine),
            RootStates::Idle(s) => s.on_entry(state_machine),
            RootStates::Starting(s) => s.on_entry(state_machine),
        }
    }

    fn on_exit(&self, state_machine: &mut StateMachine<RootComponents>) {
        match self {
            RootStates::Uninit(uninit) => uninit.on_exit(state_machine),
            RootStates::Counting(counting) => counting.on_exit(state_machine),
            RootStates::Finished(finished) => finished.on_exit(state_machine),
            RootStates::Error(error) => error.on_exit(state_machine),
            RootStates::Idle(idle) => idle.on_exit(state_machine),
            RootStates::Starting(starting) => starting.on_exit(state_machine),
        }
    }

    fn parent(&self) -> RootStates {
        match self {
            RootStates::Uninit(s) => s.parent(),
            RootStates::Idle(s) => s.parent(),
            RootStates::Starting(s) => s.parent(),
            RootStates::Counting(s) => s.parent(),
            RootStates::Finished(s) => s.parent(),
            RootStates::Error(s) => s.parent(),
        }
    }
}

impl StateEnum for RootStates {
    fn new() -> Self {
        Self::default()
    }
}

impl Default for RootStates {
    fn default() -> Self {
        RootStates::Uninit(Uninit)
    }
}
