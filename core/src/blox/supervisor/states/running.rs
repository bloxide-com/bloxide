// Copyright 2025 Bloxide, all rights reserved

use super::*;
use log::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Running;

impl<R: Runtime> State<SupervisorComponents<R>> for Running
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
        state_machine: &mut StateMachine<SupervisorComponents<R>>,
        message: SupervisorMessageSet<R>,
    ) -> Option<Transition<SupervisorStateEnum, SupervisorMessageSet<R>>> {
        let transition = match message {
            SupervisorMessageSet::SupervisorMessage(message) => match message.payload {
                SupervisorPayload::Spawn(future) => {
                    let _ = state_machine.extended_state.spawn(future).map_err(|_| {
                        error!("Failed to spawn blox");
                    });
                    None
                }
                SupervisorPayload::RequestNewStandardHandle(queue_size) => {
                    let (new_handle, rx) = state_machine
                        .extended_state
                        .request_new_standard_handle(queue_size);
                    let handle = state_machine
                        .extended_state
                        .blox
                        .get(&message.source_id())
                        .unwrap();
                    let payload = StandardPayload::StandardChannel(new_handle, rx);
                    let _ = handle.try_send(Message::new(
                        state_machine.self_handles.standard_handle.id(),
                        payload,
                    ));
                    None
                }
                _ => None,
            },
            _ => None,
        };
        transition
    }
}
