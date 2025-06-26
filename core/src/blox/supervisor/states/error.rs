// Copyright 2025 Bloxide, all rights reserved

use super::*;
use crate::prelude::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Error;

impl<R: Runtime> State<SupervisorComponents<R>> for Error
where
    R::MessageHandle<StandardPayload<R>>: Clone + Send + 'static,
    <R::MessageHandle<StandardPayload<R>> as MessageSender>::ReceiverType: Send,
    R::MessageHandle<SupervisorPayload>: Clone + Send + 'static,
    <R::MessageHandle<SupervisorPayload> as MessageSender>::ReceiverType: Send,
{
    fn parent(&self) -> SupervisorStateEnum {
        SupervisorStateEnum::Uninit(Uninit)
    }

    fn handle_message(
        &self,
        _state_machine: &mut StateMachine<SupervisorComponents<R>>,
        _message: SupervisorMessageSet<R>,
    ) -> Option<Transition<SupervisorStateEnum, SupervisorMessageSet<R>>> {
        None
    }
}
