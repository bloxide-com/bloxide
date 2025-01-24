// Copyright 2025 Bloxide, all rights reserved

use crate::core::messaging::*;
use crate::runtime::*;
use crate::std_exports::*;

// A trait to encapsulate types needed for an actor
pub trait Components {
    type StateEnum: StateEnum;
    type MessageSet: MessageSet;
    type ExtendedState: ExtendedState;
    type ActorConfig;
}
//Implement Runnable or RunnableLocal depending on if the actor implements Send
pub trait Runnable<A: Components> {
    fn run_actor(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
    fn into_request(self: Box<Self>) -> StandardPayload
    where
        Self: Send + 'static,
    {
        let closure = move || {
            Box::pin(async move {
                // We’re inside the closure, so we can now await run_actor():
                self.run_actor().await
            }) as Pin<Box<dyn Future<Output = ()> + Send + 'static>>
        };

        StandardPayload::SpawnRequest(Box::new(closure))
    }
}

pub trait RunnableLocal<A: Components> {
    fn run_actor(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + 'static>>;
    fn into_request(self: Box<Self>) -> StandardPayload
    where
        Self: Send + 'static,
    {
        let _closure = move || {
            Box::pin(async move {
                // We’re inside the closure, so we can now await run_actor():
                self.run_actor().await
            }) as Pin<Box<dyn Future<Output = ()> + 'static>>
        };

        StandardPayload::Error(Box::new("Not implemented".into()))
        //StandardPayload::SpawnLocalRequest(Box::new(closure))
        //TODO: Seperate Spawn and SpawnLocal into different Message Payload types due to Send restrictions
    }
}
pub trait StateEnum: Default + Debug {
    fn new() -> Self {
        Self::default()
    }
}
pub trait MessageSet {} //Marker trait

pub trait ExtendedState {
    fn new() -> Self;
}

//Used as Option<Transition<T>>, None = No transition
//Errors handled as transitions to a Error state
pub enum Transition<T> {
    To(T),
    Parent,
}

//The main actor struct.  Actors are differentiated by their components
//Anything that all Actors should have is stored here
pub struct Actor<S: Components> {
    //Handle has the Actor's ID, no need to duplicate it with a seperate ID field
    pub handle: StandardMessageHandle,
    pub state_machine: StateMachine<S>,
    pub config: S::ActorConfig,
}

impl<S> Actor<S>
where
    S: Components,
    S::StateEnum: State<S> + Clone + PartialEq,
{
    pub fn new(
        handle: StandardMessageHandle,
        //Initialized ExtendedState passed in here
        extended_state: S::ExtendedState,
        config: S::ActorConfig,
    ) -> Self {
        Self {
            handle: handle.clone(),
            state_machine: StateMachine::new(extended_state),
            config,
        }
    }
}

pub struct StateMachine<S: Components> {
    pub current_state: S::StateEnum,
    //ExtendedState stored here to be passed to each state
    extended_state: S::ExtendedState,
}

impl<S> StateMachine<S>
where
    S: Components,
    S::StateEnum: State<S> + Clone + PartialEq,
{
    fn new(extended_state: S::ExtendedState) -> Self {
        Self {
            current_state: S::StateEnum::new(),
            extended_state,
        }
    }

    //This is how messages get handled.  Structured so it can be called recursively for Parent transitions
    pub fn dispatch(&mut self, message: &S::MessageSet, state: &S::StateEnum)
    where
        S::StateEnum: State<S>,
    {
        //return result?

        if let Some(transition) = state.handle_message(message, &mut self.extended_state) {
            match transition {
                Transition::Parent => {
                    self.dispatch(message, &state.parent());
                }
                Transition::To(new_state) => {
                    self.change_state(new_state);
                }
            }
        }
    }

    //helper function
    fn build_state_path(&self, start_state: S::StateEnum) -> Vec<S::StateEnum> {
        let mut path = Vec::new();
        let mut current = Some(start_state);
        while let Some(s) = current {
            path.push(s.clone());
            current = match s.parent() {
                p if p == s => None, // Check if parent equals self (for Uninit state)
                p => Some(p),
            };
        }
        path.reverse();
        path
    }

    //helper function
    fn find_lca_index(&self, current_path: &[S::StateEnum], dest_path: &[S::StateEnum]) -> usize {
        let mut lca_index = 0;
        while lca_index < current_path.len().min(dest_path.len())
            && current_path[lca_index] == dest_path[lca_index]
        {
            lca_index += 1;
        }
        lca_index
    }

    //This is how states get changed
    //Traverses the correct on_exit and on_entry functions
    fn change_state(&mut self, new_state: S::StateEnum) {
        // Build current state path
        let current_path = self.build_state_path(self.current_state.clone());

        // Build destination state path
        let dest_path = self.build_state_path(new_state.clone());

        // Find LCA
        let lca_index = self.find_lca_index(&current_path, &dest_path);

        // Clone the paths to avoid borrowing issues

        // Exit from current state up to (but not including) LCA
        // TODO: Bug - Because of this, UNINIT has to call its on_exit manually on Actor initialization
        for state in current_path[lca_index..].iter().rev() {
            state.on_exit(&mut self.extended_state);
        }

        // Enter from LCA down to destination
        for state in dest_path[lca_index..].iter() {
            state.on_entry(&mut self.extended_state);
        }

        // Set new current state
        self.current_state = new_state;
    }
}

pub trait State<C: Components> {
    //Default on_entry and on_exit functions do nothing, only need to be overridden if needed
    fn on_entry(&self, _data: &mut C::ExtendedState) {}
    fn on_exit(&self, _data: &mut C::ExtendedState) {}
    //All states must have a parent set
    fn parent(&self) -> C::StateEnum {
        panic!("No parent for this state");
    }
    fn handle_message(
        &self,
        _message: &C::MessageSet,
        _data: &mut C::ExtendedState,
    ) -> Option<Transition<C::StateEnum>>;
}
