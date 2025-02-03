// Copyright 2025 Bloxide, all rights reserved

pub mod counting;
pub mod error;
pub mod finished;
pub mod idle;
pub mod not_started;
pub mod uninit;

use super::{components::*, messaging::*};
use bloxide::core::state_machine::*;
pub use {
    counting::Counting, error::Error, finished::Finished, idle::Idle, not_started::NotStarted,
    uninit::Uninit,
};

#[derive(Clone, PartialEq, Debug)]
pub enum CounterStateEnum {
    Uninit(Uninit),
    Idle(Idle),
    NotStarted(NotStarted),
    Counting(Counting),
    Finished(Finished),
    Error(Error),
}
impl Default for CounterStateEnum {
    fn default() -> Self {
        CounterStateEnum::Uninit(Uninit)
    }
}

impl StateEnum for CounterStateEnum {}

impl State<CounterComponents> for CounterStateEnum {
    fn on_entry(&self, state_machine: &mut StateMachine<CounterComponents>) {
        match self {
            CounterStateEnum::Uninit(s) => s.on_entry(state_machine),
            CounterStateEnum::NotStarted(s) => s.on_entry(state_machine),
            CounterStateEnum::Idle(s) => s.on_entry(state_machine),
            CounterStateEnum::Counting(s) => s.on_entry(state_machine),
            CounterStateEnum::Finished(s) => s.on_entry(state_machine),
            CounterStateEnum::Error(s) => s.on_entry(state_machine),
        }
    }

    fn on_exit(&self, data: &mut StateMachine<CounterComponents>) {
        match self {
            CounterStateEnum::Uninit(s) => s.on_exit(data),
            CounterStateEnum::NotStarted(s) => s.on_exit(data),
            CounterStateEnum::Idle(s) => s.on_exit(data),
            CounterStateEnum::Counting(s) => s.on_exit(data),
            CounterStateEnum::Finished(s) => s.on_exit(data),
            CounterStateEnum::Error(s) => s.on_exit(data),
        }
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<CounterComponents>,
        message: CounterMessageSet,
    ) -> Option<Transition<CounterStateEnum, CounterMessageSet>> {
        match self {
            CounterStateEnum::Uninit(s) => s.handle_message(state_machine, message),
            CounterStateEnum::NotStarted(s) => s.handle_message(state_machine, message),
            CounterStateEnum::Idle(s) => s.handle_message(state_machine, message),
            CounterStateEnum::Counting(s) => s.handle_message(state_machine, message),
            CounterStateEnum::Finished(s) => s.handle_message(state_machine, message),
            CounterStateEnum::Error(s) => s.handle_message(state_machine, message),
        }
    }

    fn parent(&self) -> CounterStateEnum {
        match self {
            CounterStateEnum::Uninit(s) => s.parent(),
            CounterStateEnum::NotStarted(s) => s.parent(),
            CounterStateEnum::Idle(s) => s.parent(),
            CounterStateEnum::Counting(s) => s.parent(),
            CounterStateEnum::Finished(s) => s.parent(),
            CounterStateEnum::Error(s) => s.parent(),
        }
    }
}
