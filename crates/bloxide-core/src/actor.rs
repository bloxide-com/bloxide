// Copyright 2025 Bloxide, all rights reserved
use core::future::poll_fn;

use crate::{
    engine::{DispatchOutcome, MachineState, StateMachine},
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

/// Run an actor until it reaches a terminal/error state or resets.
///
/// Dispatches events until `DispatchOutcome::Started` or `DispatchOutcome::Transition`
/// enters a terminal or error state, or `DispatchOutcome::Reset`/`DispatchOutcome::Stopped`
/// is observed. Suitable for dynamically spawned actors that should exit their
/// task when their work is done.
///
/// Note: This function does NOT call `machine.start()`. The actor expects lifecycle
/// commands (including Start) to arrive via the event stream.
///
/// For unsupervised actors without a lifecycle mailbox, use `run_actor_auto_start`
/// instead, which auto-starts before running.
pub async fn run_actor_to_completion<S, M>(mut machine: StateMachine<S>, mut mailboxes: M)
where
    S: MachineSpec + 'static,
    M: Mailboxes<S::Event>,
{
    loop {
        let event = poll_fn(|cx| mailboxes.poll_next(cx)).await;
        match machine.dispatch(event) {
            DispatchOutcome::Started(MachineState::State(state))
                if S::is_terminal(&state) || S::is_error(&state) =>
            {
                return;
            }
            DispatchOutcome::Transition(MachineState::State(state))
                if S::is_terminal(&state) || S::is_error(&state) =>
            {
                return;
            }
            DispatchOutcome::Done(_) => return,
            DispatchOutcome::Failed => return,
            DispatchOutcome::Reset => return,
            DispatchOutcome::Stopped => return,
            _ => {}
        }
    }
}

/// Run an unsupervised actor, auto-starting it first.
///
/// For actors that don't have a lifecycle mailbox and need to start immediately.
/// Calls `handle_lifecycle(Start)` to transition from Init, then runs like
/// `run_actor_to_completion`.
pub async fn run_actor_auto_start<S, M>(mut machine: StateMachine<S>, mut mailboxes: M)
where
    S: MachineSpec + 'static,
    M: Mailboxes<S::Event>,
{
    use crate::lifecycle::LifecycleCommand;

    // Auto-start the actor
    let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
    // Check if initial state is terminal/error
    if let DispatchOutcome::Started(MachineState::State(state)) = outcome {
        if S::is_terminal(&state) || S::is_error(&state) {
            return;
        }
    }

    // Run to completion
    loop {
        let event = poll_fn(|cx| mailboxes.poll_next(cx)).await;
        match machine.dispatch(event) {
            DispatchOutcome::Started(MachineState::State(state))
                if S::is_terminal(&state) || S::is_error(&state) =>
            {
                return;
            }
            DispatchOutcome::Transition(MachineState::State(state))
                if S::is_terminal(&state) || S::is_error(&state) =>
            {
                return;
            }
            DispatchOutcome::Done(_) => return,
            DispatchOutcome::Failed => return,
            DispatchOutcome::Reset => return,
            DispatchOutcome::Stopped => return,
            _ => {}
        }
    }
}
