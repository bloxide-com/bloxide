// Copyright 2025 Bloxide, all rights reserved
use core::future::poll_fn;

use crate::{
    engine::{DispatchOutcome, MachineState, StateMachine},
    mailboxes::Mailboxes,
    spec::MachineSpec,
};

/// Run an actor until it reaches an error state or stops.
///
/// Dispatches events until `DispatchOutcome::Failed` or `DispatchOutcome::Stopped`
/// is observed. Suitable for dynamically spawned actors that should exit their
/// task when their work is done.
///
/// Note: This function does NOT call `machine.start()`. The actor expects lifecycle
/// commands (including Start) to arrive via the event stream.
///
/// This is the **unsupervised** runner used by the test runtime. The Tokio and
/// Embassy runtimes each provide a unified `run()` function that handles both
/// supervised and unsupervised execution with optional lifecycle/abort streams.
pub async fn run_actor_to_completion<S, M>(mut machine: StateMachine<S>, mut mailboxes: M)
where
    S: MachineSpec + 'static,
    M: Mailboxes<S::Event>,
{
    loop {
        let event = match poll_fn(|cx| mailboxes.poll_next(cx)).await {
            Some(event) => event,
            None => return,
        };
        match machine.dispatch(event) {
            DispatchOutcome::Started(MachineState::State(state))
                if S::is_error(&state) =>
            {
                return;
            }
            DispatchOutcome::Transition(MachineState::State(state))
                if S::is_error(&state) =>
            {
                return;
            }
            DispatchOutcome::Failed => return,
            DispatchOutcome::Stopped => return,
            _ => {}
        }
    }
}
