// Copyright 2025 Bloxide, all rights reserved

use super::*;
#[cfg(feature = "runtime-tokio")]
use crate::runtime::*;
use crate::{core::state_machine::*, std_exports::*};
use log::*;
use embassy_executor::SendSpawner;
use embassy_executor::Spawner;


use crate::core::components::*;
#[derive(Clone, PartialEq, Debug)]
pub struct Uninit;

impl State<SupervisorComponents> for Uninit {
    fn parent(&self) -> SupervisorStateEnum {
        SupervisorStateEnum::Uninit(Uninit)
    }
    fn handle_message(
        &self,
        _state_machine: &mut StateMachine<SupervisorComponents>,
        _message: SupervisorMessageSet,
    ) -> Option<Transition<SupervisorStateEnum, SupervisorMessageSet>> {
        trace!("Uninit handle message");
        //Uninit never handles messages
        None
    }
    fn on_entry(&self, _state_machine: &mut StateMachine<SupervisorComponents>) {
        trace!("State on_entry: {:?}", self);
        info!("This is the Blox Shutdown");
    }
    fn on_exit(&self, state_machine: &mut StateMachine<SupervisorComponents>) {
        trace!("State on_exit: {:?}", self);
        info!("This is the Blox Initialization");

        if let Some(spawner) = state_machine.extended_state.spawner {
            trace!("Running root spawn function");
            if let Err(e) = spawner.spawn(root_task(state_machine.extended_state.root_spawn_fn.take().unwrap())) {
                error!("Failed to spawn root: {:?}", e);
            }
        } else {
            panic!("Root spawn function not found");
        }
    }
}

#[cfg(feature = "runtime-tokio")]
impl Uninit {
    fn spawn_root(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        spawn(future);
    }
}


#[embassy_executor::task]
pub async fn root_task(spawn_fn: RootSpawnFn) {
   spawn_fn().await;
}