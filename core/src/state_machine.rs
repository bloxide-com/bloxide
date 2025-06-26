// Copyright 2025 Bloxide, all rights reserved

use crate::prelude::*;
use log::*;

pub trait StateEnum: Default + fmt::Debug {
    fn new() -> Self {
        Self::default()
    }
}

pub trait ExtendedState {
    type InitArgs;
    fn new(args: Self::InitArgs) -> Self;
}

/// Used as `Option<Transition<T>>`, `None` = No transition
/// Errors handled as transitions to a Error state
pub enum Transition<T, M> {
    To(T),
    Parent(M),
}

pub struct StateMachine<C: Components> {
    pub current_state: C::States,
    // ExtendedState stored here to be passed to each state
    pub extended_state: C::ExtendedState,
    pub self_handles: C::Handles,
}

impl<C> StateMachine<C>
where
    C: Components,
    C::States: State<C> + Clone + PartialEq + Default,
{
    pub fn new(extended_state: C::ExtendedState, self_handles: C::Handles) -> Self {
        Self {
            current_state: C::States::default(),
            extended_state,
            self_handles,
        }
    }

    // Initializes the state machine, performs the Uninit state transition
    pub fn init(&mut self, uninit: &C::States, entry_point: &C::States) {
        uninit.on_exit(self);
        self.change_state(entry_point.clone());
    }

    // This is how messages get handled.  Structured so it can be called recursively for Parent message handling
    pub fn dispatch(&mut self, message: C::MessageSet, state: &C::States)
    where
        C::States: State<C>,
    {
        let transition = state.handle_message(self, message);
        match transition {
            Some(Transition::Parent(message)) => {
                trace!("Transitioning to parent state");
                self.dispatch(message, &state.parent());
            }
            Some(Transition::To(new_state)) => {
                trace!("Transitioning to state: {:?}", new_state);
                self.change_state(new_state);
            }
            _ => {
                // Do nothing if transition is None regardless of message
            }
        }
    }

    fn build_state_path(&self, start_state: C::States) -> Vec<C::States> {
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

    fn find_lca_index(&self, current_path: &[C::States], dest_path: &[C::States]) -> usize {
        let mut lca_index = 0;
        while lca_index < current_path.len().min(dest_path.len())
            && current_path[lca_index] == dest_path[lca_index]
        {
            lca_index += 1;
        }
        lca_index
    }

    // This is how states get changed
    // Traverses the on_exit and on_entry functions
    fn change_state(&mut self, new_state: C::States) {
        // Build current state path
        let current_path = self.build_state_path(self.current_state.clone());
        trace!("Current state path: {:?}", current_path);

        // Build destination state path
        let dest_path = self.build_state_path(new_state.clone());
        trace!("Destination state path: {:?}", dest_path);

        // Find LCA
        let lca_index = self.find_lca_index(&current_path, &dest_path);
        trace!("LCA index: {:?}", lca_index);

        // Exit from current state up to (but not including) LCA
        for state in current_path[lca_index..].iter().rev() {
            state.on_exit(self);
        }

        // Enter from LCA down to destination
        for state in dest_path[lca_index..].iter() {
            state.on_entry(self);
        }

        // Set new current state
        self.current_state = new_state;
    }
}

pub trait State<C: Components>: fmt::Debug + 'static {
    // Default on_entry and on_exit functions do nothing, only need to be overridden if needed
    fn on_entry(&self, _state_machine: &mut StateMachine<C>) {
        trace!("State on_entry: {:?}", self);
    }

    fn on_exit(&self, _state_machine: &mut StateMachine<C>) {
        trace!("State on_exit: {:?}", self);
    }

    // All states must have a parent state
    fn parent(&self) -> C::States {
        panic!("No parent for this state");
    }

    fn handle_message(
        &self,
        state_machine: &mut StateMachine<C>,
        message: C::MessageSet,
    ) -> Option<Transition<C::States, C::MessageSet>>;
}
