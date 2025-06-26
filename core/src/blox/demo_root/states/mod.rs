// Copyright 2025 Bloxide, all rights reserved

pub mod counting;
pub mod error;
pub mod finished;
pub mod idle;
pub mod starting;
pub mod uninit;

use super::components::RootComponents;
use crate::{blox::demo_counter::messaging::CounterPayload, components::Runtime, prelude::*};
use counting::Counting;
use error::Error;
use finished::Finished;
use idle::Idle;
pub use starting::Starting;
pub use uninit::Uninit;

#[derive(Clone, PartialEq, Debug)]
pub enum RootStates {
    Uninit(Uninit),
    Idle(Idle),
    Starting(Starting),
    Counting(Counting),
    Finished(Finished),
    Error(Error),
}

impl<R: Runtime> State<RootComponents<R>> for RootStates
where
    R::MessageHandle<StandardPayload<R>>: Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    R::MessageHandle<CounterPayload>: Clone + Send + 'static,
    <R::MessageHandle<CounterPayload> as MessageSender>::ReceiverType: Send,
{
    fn handle_message(
        &self,
        state_machine: &mut StateMachine<RootComponents<R>>,
        message: <RootComponents<R> as Components>::MessageSet,
    ) -> Option<Transition<RootStates, <RootComponents<R> as Components>::MessageSet>> {
        match self {
            RootStates::Uninit(s) => s.handle_message(state_machine, message),
            RootStates::Counting(s) => s.handle_message(state_machine, message),
            RootStates::Finished(s) => s.handle_message(state_machine, message),
            RootStates::Error(s) => s.handle_message(state_machine, message),
            RootStates::Idle(s) => s.handle_message(state_machine, message),
            RootStates::Starting(s) => s.handle_message(state_machine, message),
        }
    }

    fn on_entry(&self, state_machine: &mut StateMachine<RootComponents<R>>) {
        match self {
            RootStates::Uninit(s) => s.on_entry(state_machine),
            RootStates::Counting(s) => s.on_entry(state_machine),
            RootStates::Finished(s) => s.on_entry(state_machine),
            RootStates::Error(s) => s.on_entry(state_machine),
            RootStates::Idle(s) => s.on_entry(state_machine),
            RootStates::Starting(s) => s.on_entry(state_machine),
        }
    }

    fn on_exit(&self, state_machine: &mut StateMachine<RootComponents<R>>) {
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
            RootStates::Uninit(s) => <Uninit as State<RootComponents<R>>>::parent(s),
            RootStates::Idle(s) => <Idle as State<RootComponents<R>>>::parent(s),
            RootStates::Starting(s) => <Starting as State<RootComponents<R>>>::parent(s),
            RootStates::Counting(s) => <Counting as State<RootComponents<R>>>::parent(s),
            RootStates::Finished(s) => <Finished as State<RootComponents<R>>>::parent(s),
            RootStates::Error(s) => <Error as State<RootComponents<R>>>::parent(s),
        }
    }
}

impl StateEnum for RootStates {}

impl Default for RootStates {
    fn default() -> Self {
        RootStates::Uninit(Uninit)
    }
}
