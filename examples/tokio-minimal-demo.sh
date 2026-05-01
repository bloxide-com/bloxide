#!/bin/bash
set -e

DEMO="demo/tokio-minimal"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
rm -rf "$REPO_ROOT/$DEMO"
mkdir -p "$REPO_ROOT/$DEMO"

cd "$REPO_ROOT/$DEMO"

cat > Cargo.toml <<'WORKSPACE'
[workspace]
members = ["apps/tokio-minimal"]
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"

[workspace.dependencies]
bloxide-core        = { path = "../../../crates/bloxide-core" }
bloxide-tokio       = { path = "../../../runtimes/bloxide-tokio" }
bloxide-macros      = { path = "../../../crates/bloxide-macros" }
bloxide-log         = { path = "../../../crates/bloxide-log", features = ["log"] }
counter-messages    = { path = "../../../crates/messages/counter-messages" }
counter-actions     = { path = "../../../crates/actions/counter-actions" }
counter-blox        = { path = "../../../crates/bloxes/counter" }
counter-demo-impl   = { path = "../../../crates/impl/counter-demo-impl" }

[profile.dev]
panic = "abort"
WORKSPACE

# ── Binary app crate ────────────────────────────────────────────────────────
mkdir -p apps/tokio-minimal/src

cat > apps/tokio-minimal/Cargo.toml <<'CRATE'
[package]
name = "tokio-minimal"
version.workspace = true
edition.workspace = true
publish = false

[dependencies]
bloxide-core      = { workspace = true, features = ["std"] }
bloxide-tokio     = { workspace = true }
bloxide-log       = { workspace = true }
counter-blox      = { workspace = true }
counter-messages  = { workspace = true }
counter-demo-impl = { workspace = true }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-log = "0.2"
CRATE

cat > apps/tokio-minimal/src/main.rs <<'MAIN'
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_tokio::prelude::*;
use counter_blox::prelude::*;
use counter_demo_impl::CounterBehavior;
use counter_messages::prelude::*;

bloxide_tokio::actor_task_supervised!(counter_task, CounterSpec<TokioRuntime, CounterBehavior>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

#[tokio::main]
async fn main() {
    let ((counter_ref,), counter_mbox) = bloxide_tokio::channels! {
        CounterMsg(8),
    };
    let counter_id = counter_ref.id();

    let machine = StateMachine::<CounterSpec<TokioRuntime, CounterBehavior>>::new(CounterCtx::new(
        counter_id,
        CounterBehavior::default(),
    ));

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

    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));

    tracing::info!(counter_id, sup_id, "counter and supervisor created");

    counter_ref
        .try_send(counter_id, CounterMsg::Tick(Tick))
        .expect("counter mailbox should accept the first tick");
    counter_ref
        .try_send(counter_id, CounterMsg::Tick(Tick))
        .expect("counter mailbox should accept the second tick");

    supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;

    println!("tokio-minimal-demo complete");
}
MAIN

cargo run -p tokio-minimal
