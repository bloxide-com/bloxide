// Copyright 2025 Bloxide, all rights reserved
#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use bloxide_core::{mailboxes::Mailboxes, spec::MachineSpec, StateMachine};
use core::future::poll_fn;

pub use bloxide_core::{run_actor, run_actor_auto_start, run_actor_to_completion};

#[doc(hidden)]
pub use bloxide_macros::channels as __channels_proc_macro;
#[doc(hidden)]
pub use bloxide_macros::next_actor_id as __next_actor_id_proc_macro;

pub mod channel;
pub mod mailbox;
pub mod prelude;
pub mod supervision;
pub mod timer;

pub use bloxide_core::{ChildLifecycleEvent, LifecycleCommand};
pub use channel::{EmbassySender, EmbassyStream, EmbassyTrySendError};
pub use supervision::{run_supervised_actor, ChildGroupBuilder};

// ── EmbassyRuntime ────────────────────────────────────────────────────────────

/// The Embassy runtime capability handle (zero-sized type).
#[derive(Clone, Copy)]
pub struct EmbassyRuntime;

// ── next_actor_id! macro ──────────────────────────────────────────────────────

/// Allocate a compile-time actor ID from the same counter used by `channels!`.
#[macro_export]
macro_rules! next_actor_id {
    () => {
        $crate::__next_actor_id_proc_macro!()
    };
}

// ── channels! macro ───────────────────────────────────────────────────────────

/// Create all channels for an actor in one call.
///
/// Takes a comma-separated list of `MessageType(capacity)` pairs and returns
/// `(refs_tuple, mailboxes_tuple)`.
#[macro_export]
macro_rules! channels {
    ($($tt:tt)*) => {
        $crate::__channels_proc_macro!($crate::EmbassyRuntime; $($tt)*)
    };
}

// ── actor_task! macro ─────────────────────────────────────────────────────────

/// Generate an `#[embassy_executor::task]` wrapper for a bloxide actor.
#[macro_export]
macro_rules! actor_task {
    ($name:ident, $spec:ty $(,)?) => {
        #[embassy_executor::task]
        async fn $name(
            machine: ::bloxide_core::StateMachine<$spec>,
            mailboxes: <$spec as ::bloxide_core::spec::MachineSpec>::Mailboxes<
                $crate::EmbassyRuntime,
            >,
        ) {
            $crate::run_actor(machine, mailboxes).await;
        }
    };
}

// ── actor_task_supervised! macro ──────────────────────────────────────────────

/// Generate an `#[embassy_executor::task]` wrapper for a supervised bloxide actor.
#[macro_export]
macro_rules! actor_task_supervised {
    ($name:ident, $spec:ty $(,)?) => {
        #[embassy_executor::task]
        async fn $name(
            machine: ::bloxide_core::StateMachine<$spec>,
            domain_mailboxes: <$spec as ::bloxide_core::spec::MachineSpec>::Mailboxes<
                $crate::EmbassyRuntime,
            >,
            lifecycle_rx: $crate::EmbassyStream<$crate::LifecycleCommand>,
            actor_id: ::bloxide_core::messaging::ActorId,
            supervisor_notify: $crate::EmbassySender<$crate::ChildLifecycleEvent>,
        ) {
            $crate::supervision::run_supervised_actor(
                machine,
                domain_mailboxes,
                lifecycle_rx,
                actor_id,
                supervisor_notify,
            )
            .await;
        }
    };
}

// ── root_task! macro ──────────────────────────────────────────────────────────

/// Generate an `#[embassy_executor::task]` wrapper for a top-level supervisor.
///
/// When the supervisor's spec transitions to Init via `Guard::Reset`, the
/// runtime returns and the optional exit expression is executed.
///
/// # Usage
///
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// // On std targets — exit the process after shutdown:
/// root_task!(supervisor_task, SuperSpec<EmbassyRuntime>, std::process::exit(0));
///
/// // On embedded targets — just return (task ends, executor continues):
/// root_task!(supervisor_task, SuperSpec<EmbassyRuntime>);
/// ```
#[macro_export]
macro_rules! root_task {
    ($name:ident, $spec:ty, $on_done:expr $(,)?) => {
        #[embassy_executor::task]
        async fn $name(
            machine: ::bloxide_core::StateMachine<$spec>,
            mailboxes: <$spec as ::bloxide_core::spec::MachineSpec>::Mailboxes<
                $crate::EmbassyRuntime,
            >,
        ) {
            $crate::run_root(machine, mailboxes).await;
            $on_done
        }
    };
    ($name:ident, $spec:ty $(,)?) => {
        #[embassy_executor::task]
        async fn $name(
            machine: ::bloxide_core::StateMachine<$spec>,
            mailboxes: <$spec as ::bloxide_core::spec::MachineSpec>::Mailboxes<
                $crate::EmbassyRuntime,
            >,
        ) {
            $crate::run_root(machine, mailboxes).await;
        }
    };
}

// ── timer_task! macro ─────────────────────────────────────────────────────────

/// Generate an `#[embassy_executor::task]` for the timer service.
#[macro_export]
macro_rules! timer_task {
    ($name:ident) => {
        #[embassy_executor::task]
        async fn $name(stream: $crate::EmbassyStream<::bloxide_timer::TimerCommand>) {
            <$crate::EmbassyRuntime as ::bloxide_timer::TimerService>::run_timer_service(stream)
                .await;
        }
    };
}

// ── spawn_timer! macro ────────────────────────────────────────────────────────

/// Spawn the timer service and return the `ActorRef<TimerCommand>` for it.
#[macro_export]
macro_rules! spawn_timer {
    ($spawner:expr, $task_fn:ident, $capacity:expr) => {{
        let ((timer_ref,), (timer_stream,)) =
            $crate::__channels_proc_macro!($crate::EmbassyRuntime; ::bloxide_timer::TimerCommand($capacity));
        $spawner.must_spawn($task_fn(timer_stream));
        timer_ref
    }};
}

// ── spawn_child! macro ────────────────────────────────────────────────────────

/// Spawn a supervised child actor task.
///
/// Creates the per-child lifecycle channel, registers the child in the
/// `ChildGroupBuilder`, and spawns the Embassy task with the lifecycle
/// arguments injected automatically.
#[macro_export]
macro_rules! spawn_child {
    ($spawner:expr, $builder:expr, $task_fn:ident($machine:expr, $mbox:expr, $id:expr), $policy:expr) => {{
        let (lc_rx, sup_notify) = $builder.add_child($id, $policy);
        $spawner.must_spawn($task_fn($machine, $mbox, lc_rx, $id, sup_notify));
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
