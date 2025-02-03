// Copyright 2025 Bloxide, all rights reserved

use super::*;
use crate::runtime::*;
use crate::{
    core::{messaging::*, state_machine::*},
    std_exports::*,
};
use log::*;
use core::ops::DerefMut;

#[derive(Clone, PartialEq, Debug)]
pub struct Running;

impl State<SupervisorComponents> for Running {
    fn parent(&self) -> SupervisorStateEnum {
        SupervisorStateEnum::Uninit(Uninit)
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<SupervisorComponents>,
        message: SupervisorMessageSet,
    ) -> Option<Transition<SupervisorStateEnum, SupervisorMessageSet>> {
        trace!("[Running] handle_message: {:?}", message);
        let transition = match message {
            SupervisorMessageSet::SupervisorMessage(message) => match message.payload {
                SupervisorPayload::Spawn(spawn_fn) => {
                    if let Some(spawner) = state_machine.extended_state.spawner {
                        if let Err(e) = spawner.spawn(spawn_blox(spawn_fn)) {
                            error!("Failed to spawn blox: {:?}", e);
                        }
                    }
                    
                    None
                }
                SupervisorPayload::RequestNewStandardHandle(queue_size) => {
                    let (new_handle, rx) = state_machine
                        .extended_state
                        .request_new_standard_handle(queue_size);
                    // get handle for the id in the message to send the response
                    let handle = state_machine
                        .extended_state
                        .blox
                        .get(&message.source_id())
                        .unwrap();
                    if let Err(e) = handle.try_send(Message::new(
                        state_machine.self_handles.standard_handle.dest_id,
                        StandardPayload::StandardChannel(new_handle, rx),
                    )) {
                        error!("Supervisor Failed to send message: {:?}", e);
                    }
                    None
                }
                _ => None,
            },
            _ => None,
        };
        transition
    }
}

#[cfg(feature = "runtime-tokio")]
impl Running {
    fn spawn_blox(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        spawn(future);
    }
}

#[cfg(feature = "runtime-embassy")]
#[embassy_executor::task]
async fn spawn_blox(future: SpawnFn) {
    future().await;
    info!("Blox finished");
}

pub type SpawnFn = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;