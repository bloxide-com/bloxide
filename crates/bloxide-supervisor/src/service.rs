// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{engine::StateMachine, messaging::ActorId, spec::MachineSpec};

use crate::lifecycle::{ChildLifecycleEvent, LifecycleCommand};

/// Trait that runtimes implement to provide supervised actor run loops.
///
/// The implementation merges a lifecycle command stream with the actor's
/// domain mailboxes, using the runtime's async combinators (e.g. `select`).
/// After each dispatch, it observes `DispatchOutcome` and sends
/// `ChildLifecycleEvent` to the supervisor's notification channel.
///
/// Blox crates never use this trait as a bound. It is used by wiring
/// macros (e.g. `actor_task_supervised!`) and enforces that every runtime
/// provides a compatible supervised actor implementation.
#[allow(async_fn_in_trait)]
pub trait SupervisedRunLoop: bloxide_core::capability::BloxRuntime {
    /// Run a supervised actor until it is stopped.
    async fn run_supervised_actor<S: MachineSpec + 'static>(
        machine: StateMachine<S>,
        domain_mailboxes: S::Mailboxes<Self>,
        lifecycle_stream: Self::Stream<LifecycleCommand>,
        actor_id: ActorId,
        supervisor_notify: Self::Sender<ChildLifecycleEvent>,
    );
}
