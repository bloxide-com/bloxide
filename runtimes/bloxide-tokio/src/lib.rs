// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{mailboxes::Mailboxes, spec::MachineSpec, StateMachine};
use core::future::poll_fn;

pub use bloxide_core::{run_actor, run_actor_to_completion};
pub use bloxide_spawn::SpawnCap;

#[doc(hidden)]
pub use bloxide_macros::dyn_channels as __dyn_channels_proc_macro;
#[doc(hidden)]
pub use bloxide_macros::next_actor_id as __next_actor_id_proc_macro;

pub mod channel;
pub mod mailbox;
pub mod prelude;
pub mod spawn;
pub mod supervision;
pub mod timer;

pub use bloxide_supervisor::{ChildLifecycleEvent, LifecycleCommand, SupervisedRunLoop};
pub use channel::{TokioSender, TokioStream, TokioTrySendError};
pub use supervision::ChildGroupBuilder;

// ── TokioRuntime ──────────────────────────────────────────────────────────────

/// The Tokio runtime capability handle (zero-sized type).
#[derive(Clone, Copy)]
pub struct TokioRuntime;

// ── next_actor_id! macro ──────────────────────────────────────────────────────

/// Allocate a compile-time actor ID from the same counter used by `channels!`.
#[macro_export]
macro_rules! next_actor_id {
    () => {
        $crate::__next_actor_id_proc_macro!()
    };
}

// ── channels! macro ───────────────────────────────────────────────────────────

/// Create all channels for an actor in one call using Tokio's dynamic channels.
///
/// Takes a comma-separated list of `MessageType(capacity)` pairs and returns
/// `(refs_tuple, mailboxes_tuple)`.
#[macro_export]
macro_rules! channels {
    ($($tt:tt)*) => {
        $crate::__dyn_channels_proc_macro!($crate::TokioRuntime; $($tt)*)
    };
}

// ── spawn_timer! macro ────────────────────────────────────────────────────────

/// Spawn the timer service and return the `ActorRef<TimerCommand>` for it.
///
/// The timer task is spawned as a Tokio task and runs until it receives a
/// `TimerCommand::Shutdown` message.
#[macro_export]
macro_rules! spawn_timer {
    ($capacity:expr) => {{
        let ((timer_ref,), (timer_stream,)) =
            $crate::__dyn_channels_proc_macro!($crate::TokioRuntime; ::bloxide_timer::TimerCommand($capacity));
        tokio::spawn(
            <$crate::TokioRuntime as ::bloxide_timer::TimerService>::run_timer_service(
                timer_stream,
            ),
        );
        timer_ref
    }};
}

// ── spawn_child! macro ────────────────────────────────────────────────────────

/// Spawn a supervised child actor task using Tokio.
///
/// Creates the per-child lifecycle channel, registers the child in the
/// `ChildGroupBuilder`, and spawns the task with lifecycle arguments injected.
///
/// Unlike the Embassy version, there is no `spawner` parameter — Tokio tasks
/// are spawned directly via `tokio::spawn`.
#[macro_export]
macro_rules! spawn_child {
    ($builder:expr, $task_fn:ident($machine:expr, $mbox:expr, $id:expr), $policy:expr) => {{
        let (lc_rx, sup_notify) = $builder.add_child($id, $policy);
        tokio::spawn($task_fn($machine, $mbox, lc_rx, $id, sup_notify));
    }};
}

// ── Actor run loop ────────────────────────────────────────────────────────────

/// Run the program's top-level supervisor.
///
/// Like `run_actor`, dispatches events from `mailboxes` to `machine` in
/// run-to-completion order. When `DispatchOutcome::Reset` is observed,
/// the function returns so the caller can terminate.
pub async fn run_root<S, M>(mut machine: StateMachine<S>, mut mailboxes: M)
where
    S: MachineSpec + 'static,
    M: Mailboxes<S::Event>,
{
    use bloxide_core::engine::DispatchOutcome;
    loop {
        let event = poll_fn(|cx| mailboxes.poll_next(cx)).await;
        if let DispatchOutcome::Reset = machine.dispatch(event) {
            return;
        }
    }
}
