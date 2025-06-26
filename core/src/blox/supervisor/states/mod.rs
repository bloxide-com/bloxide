// Copyright 2025 Bloxide, all rights reserved

pub mod error;
pub mod running;
pub mod uninit;

use super::{components::*, messaging::*};
use crate::prelude::*;
pub use {error::*, running::*, uninit::*};

#[derive(Clone, PartialEq, Debug)]
pub enum SupervisorStateEnum {
    Uninit(Uninit),
    Running(Running),
    Error(Error),
}
impl Default for SupervisorStateEnum {
    fn default() -> Self {
        SupervisorStateEnum::Uninit(Uninit)
    }
}
impl StateEnum for SupervisorStateEnum {}

impl<R: Runtime> State<SupervisorComponents<R>> for SupervisorStateEnum
where
    R::MessageHandle<StandardPayload<R>>: Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    R::MessageHandle<SupervisorPayload>: Clone + Send + 'static,
    <R::MessageHandle<SupervisorPayload> as MessageSender>::ReceiverType: Send,
{
    fn on_entry(&self, state_machine: &mut StateMachine<SupervisorComponents<R>>) {
        match self {
            SupervisorStateEnum::Uninit(s) => s.on_entry(state_machine),
            SupervisorStateEnum::Running(s) => s.on_entry(state_machine),
            SupervisorStateEnum::Error(s) => s.on_entry(state_machine),
        }
    }

    fn on_exit(&self, state_machine: &mut StateMachine<SupervisorComponents<R>>) {
        match self {
            SupervisorStateEnum::Uninit(s) => s.on_exit(state_machine),
            SupervisorStateEnum::Running(s) => s.on_exit(state_machine),
            SupervisorStateEnum::Error(s) => s.on_exit(state_machine),
        }
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<SupervisorComponents<R>>,
        message: SupervisorMessageSet<R>,
    ) -> Option<Transition<SupervisorStateEnum, SupervisorMessageSet<R>>> {
        match self {
            SupervisorStateEnum::Uninit(s) => s.handle_message(state_machine, message),
            SupervisorStateEnum::Running(s) => s.handle_message(state_machine, message),
            SupervisorStateEnum::Error(s) => s.handle_message(state_machine, message),
        }
    }

    fn parent(&self) -> SupervisorStateEnum {
        match self {
            SupervisorStateEnum::Uninit(s) => <Uninit as State<SupervisorComponents<R>>>::parent(s),
            SupervisorStateEnum::Running(s) => {
                <Running as State<SupervisorComponents<R>>>::parent(s)
            }
            SupervisorStateEnum::Error(s) => <Error as State<SupervisorComponents<R>>>::parent(s),
        }
    }
}
