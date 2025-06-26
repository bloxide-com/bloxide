// Copyright 2025 Bloxide, all rights reserved

use super::*;

use log::*;
#[derive(Clone, PartialEq, Debug)]
pub struct Uninit;

impl<R: Runtime> State<SupervisorComponents<R>> for Uninit
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
        trace!("Uninit handle message");
        //Uninit never handles messages
        None
    }
    fn on_entry(&self, _data: &mut StateMachine<SupervisorComponents<R>>) {
        trace!("State on_entry: {:?}", self);
        info!("This is the Blox Shutdown");
    }
    fn on_exit(&self, data: &mut StateMachine<SupervisorComponents<R>>) {
        trace!("State on_exit: {:?}", self);
        info!("This is the Blox Initialization");

        if let Some(future) = data.extended_state.root_future.take() {
            trace!("Running root spawn function");
            R::spawn(future);
        } else {
            panic!("Root spawn function not found");
        }
    }
}
