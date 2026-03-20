// Copyright 2025 Bloxide, all rights reserved
//! Minimal layered Tokio demo for first-time Bloxide users.
//!
//! This binary is intentionally limited to Layer 5 (wiring). Domain pieces live in:
//! - Layer 1: `counter-messages`
//! - Layer 2: `counter-actions`
//! - Layer 3: `counter-demo-impl`
//! - Layer 4: `counter-blox`
//!
//! Demonstrates supervision: the counter actor is supervised with a Stop policy,
//! meaning it will be cleanly stopped when it reaches a terminal state.

use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_tokio::prelude::*;
use counter_blox::prelude::*;
use counter_demo_impl::CounterBehavior;
use counter_messages::prelude::*;

bloxide_tokio::actor_task_supervised!(counter_task, CounterSpec<TokioRuntime, CounterBehavior>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

#[tokio::main]
async fn main() {
    // Create the counter's mailbox.
    let ((counter_ref,), counter_mbox) = bloxide_tokio::channels! {
        CounterMsg(8),
    };
    let counter_id = counter_ref.id();

    // Build the counter machine.
    let machine = StateMachine::<CounterSpec<TokioRuntime, CounterBehavior>>::new(CounterCtx::new(
        counter_id,
        CounterBehavior::default(),
    ));

    // Set up supervision with Stop policy (clean shutdown when done).
    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
    bloxide_tokio::spawn_child!(
        group,
        counter_task(machine, counter_mbox, counter_id),
        ChildPolicy::Stop
    );
    let _sup_control_ref = group.control_ref();
    let _sup_notify = group.notify_sender();
    let sup_id = bloxide_tokio::next_actor_id!();
    let (children, sup_notify_rx, sup_control_rx) = group.finish();

    // Build and start the supervisor.
    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));

    tracing::info!(counter_id, sup_id, "counter and supervisor created");

    // Send ticks to the counter.
    counter_ref
        .try_send(counter_id, CounterMsg::Tick(Tick))
        .expect("counter mailbox should accept the first tick");
    counter_ref
        .try_send(counter_id, CounterMsg::Tick(Tick))
        .expect("counter mailbox should accept the second tick");

    // Run the supervisor until the counter reaches Done and triggers shutdown.
    supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;

    println!("tokio-minimal-demo complete");
}
