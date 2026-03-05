// Copyright 2025 Bloxide, all rights reserved
use core::future::poll_fn;

use crate::{
    engine::{DispatchOutcome, StateMachine},
    mailboxes::Mailboxes,
    spec::MachineSpec,
};

/// Run an actor forever (unsupervised, non-terminating).
///
/// Selects the next event from `mailboxes` (in priority order — index 0 wins
/// on ties) and dispatches it to `machine` (run-to-completion semantics).
/// This function never returns under normal operation.
pub async fn run_actor<S, M>(mut machine: StateMachine<S>, mut mailboxes: M)
where
    S: MachineSpec + 'static,
    M: Mailboxes<S::Event>,
{
    loop {
        let event = poll_fn(|cx| mailboxes.poll_next(cx)).await;
        machine.dispatch(event);
    }
}

/// Start an actor and run until it reaches a terminal/error state or resets.
///
/// Calls `machine.start()` to transition out of Init, then dispatches events
/// until `DispatchOutcome::Transition` into a terminal or error state, or
/// `DispatchOutcome::Reset`. Suitable for dynamically spawned actors that
/// should exit their task when their work is done.
pub async fn run_actor_to_completion<S, M>(mut machine: StateMachine<S>, mut mailboxes: M)
where
    S: MachineSpec + 'static,
    M: Mailboxes<S::Event>,
{
    machine.start();
    loop {
        let event = poll_fn(|cx| mailboxes.poll_next(cx)).await;
        match machine.dispatch(event) {
            DispatchOutcome::Transition(state) if S::is_terminal(&state) || S::is_error(&state) => {
                return;
            }
            DispatchOutcome::Reset => return,
            _ => {}
        }
    }
}
