// Copyright 2025 Bloxide, all rights reserved

use crate::runtime::*;
use crate::std_exports::*;

pub trait Actor: Send + 'static {
    type ActorData: ActorData;
    type StateEnum: State<ActorData = Self::ActorData>;
    type MessageSet;
    type ActorError;
    type InitArgs;

    fn new(id: u16, handle: StandardMessageHandle, args: Self::InitArgs) -> Self;
    fn current_state(&self) -> &'static Self::StateEnum;
    fn self_standard_handle(&self) -> &StandardMessageHandle;

    //This function changes the state of the actor
    //It finds the LCA (lowest common ancestor)
    //and then exits and enters the states in order
    //A state change return of "no change" does not run this function
    //A state change to the same state DOES run this function, and run's its own on_exit and on_entry

    fn change_state(&mut self, state: &'static Self::StateEnum) {
        // Build current state path
        let mut current_path = Vec::new();
        let mut current = Some(self.current_state());
        while let Some(s) = current {
            current_path.push(s);
            current = match s.parent() {
                p if p == s => None, // Check if parent equals self (for Uninit state)
                p => Some(p),
            };
        }
        current_path.reverse();

        // Build destination state path
        let mut dest_path = Vec::new();
        let mut current = Some(state);
        while let Some(s) = current {
            dest_path.push(s);
            current = match s.parent() {
                p if p == s => None,
                p => Some(p),
            };
        }
        dest_path.reverse();

        // Find LCA
        let mut lca_index = 0;
        while lca_index < current_path.len().min(dest_path.len())
            && current_path[lca_index] == dest_path[lca_index]
        {
            lca_index += 1;
        }

        // Exit from current state up to (but not including) LCA
        for state in current_path[lca_index..].iter().rev() {
            state.on_exit(self.actor_data_mut());
        }

        // Enter from LCA down to destination
        for state in dest_path[lca_index..].iter() {
            state.on_entry(self.actor_data_mut());
        }

        self.set_current_state(state);
    }
    fn actor_data_mut(&mut self) -> &mut Self::ActorData;
    fn set_current_state(&mut self, state: &'static Self::StateEnum);
    fn id(&self) -> u16;

    //The main message handler for the actor
    //This is not async.  This is because we run to completion for each message
    //The async part is handled in the message receiver, which is outside the scope of the HSM
    fn handle_message(&mut self, message: &Self::MessageSet);

    //error handler for the actor
    fn handle_error(&mut self, error: Self::ActorError);
}

//This is an Actor's "extended state" or "context"
pub trait ActorData: Send + 'static {
    type InitArgs;
    fn new(args: Self::InitArgs) -> Self;
}

#[derive(Debug)]
pub enum ActorError {
    SpawnError(String),
    RegisterError(String),
    SendError(String),
}

pub trait State: Send + Sized + PartialEq + 'static {
    type StateChange;
    type StateError;
    type ActorData: ActorData;

    fn new() -> Self;
    fn on_entry(&self, data: &mut Self::ActorData);
    fn on_exit(&self, data: &mut Self::ActorData);
    fn on_init(&self, data: &mut Self::ActorData) -> Result<Self::StateChange, Self::StateError>;
    fn on_standard_error(&self, error: &str) -> Result<Self::StateChange, Self::StateError>;
    fn parent(&self) -> &'static Self;
}

pub trait ActorSpawner: core::fmt::Debug + Send + 'static {
    fn spawn(&self, id: u16) -> Result<StandardMessageHandle, ActorError>;
}
