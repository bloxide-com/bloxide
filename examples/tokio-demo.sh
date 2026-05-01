#!/bin/bash
set -e

DEMO="demo/tokio-demo"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
rm -rf "$REPO_ROOT/$DEMO"
mkdir -p "$REPO_ROOT/$DEMO"

cd "$REPO_ROOT/$DEMO"

cat > Cargo.toml <<'WORKSPACE'
[workspace]
members = ["apps/tokio-demo"]
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"

[workspace.dependencies]
bloxide-core         = { path = "../../../crates/bloxide-core" }
bloxide-tokio        = { path = "../../../runtimes/bloxide-tokio" }
bloxide-macros       = { path = "../../../crates/bloxide-macros" }
bloxide-log          = { path = "../../../crates/bloxide-log", features = ["log"] }
bloxide-timer        = { path = "../../../crates/bloxide-timer" }
ping-pong-messages   = { path = "../../../crates/messages/ping-pong-messages" }
ping-pong-actions    = { path = "../../../crates/actions/ping-pong-actions" }
ping-blox            = { path = "../../../crates/bloxes/ping" }
pong-blox            = { path = "../../../crates/bloxes/pong" }
embassy-demo-impl    = { path = "../../../crates/impl/embassy-demo-impl" }

[profile.dev]
panic = "abort"
WORKSPACE

# ── Binary app crate ────────────────────────────────────────────────────────
mkdir -p apps/tokio-demo/src

cat > apps/tokio-demo/Cargo.toml <<'CRATE'
[package]
name = "tokio-demo"
version.workspace = true
edition.workspace = true
publish = false

[dependencies]
bloxide-core       = { workspace = true, features = ["std"] }
bloxide-tokio      = { workspace = true }
bloxide-log        = { workspace = true }
bloxide-timer      = { workspace = true, features = ["std"] }
ping-blox          = { workspace = true }
pong-blox          = { workspace = true }
ping-pong-messages = { workspace = true }
embassy-demo-impl  = { workspace = true }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-log = "0.2"
CRATE

cat > apps/tokio-demo/src/main.rs <<'MAIN'
use bloxide_tokio::prelude::*;
use embassy_demo_impl::PingBehavior;
use ping_blox::prelude::*;
use ping_pong_messages::prelude::*;
use pong_blox::prelude::*;
use std::time::Duration;

use bloxide_core::lifecycle::LifecycleCommand;

bloxide_tokio::actor_task_supervised!(ping_task, PingSpec<TokioRuntime, PingBehavior>);
bloxide_tokio::actor_task_supervised!(pong_task, PongSpec<TokioRuntime>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

#[tokio::main]
async fn main() {
    tracing_log::LogTracer::init().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("trace")),
        )
        .try_init()
        .ok();

    let timer_ref = bloxide_tokio::spawn_timer!(8);

    let ((ping_ref,), ping_mbox) = bloxide_tokio::channels! {
        PingPongMsg(16),
    };
    let ping_id = ping_ref.id();

    let ((pong_ref,), pong_mbox) = bloxide_tokio::channels! {
        PingPongMsg(16),
    };
    let pong_id = pong_ref.id();

    tracing::info!(ping_id, pong_id, "setup");

    let ping_ctx = PingCtx::new(
        ping_id,
        pong_ref.clone(),
        ping_ref.clone(),
        timer_ref,
        PingBehavior::default(),
    );
    let pong_ctx = PongCtx::new(pong_id, ping_ref.clone());

    let ping_machine = StateMachine::new(ping_ctx);
    let pong_machine = StateMachine::new(pong_ctx);

    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
    bloxide_tokio::spawn_child!(
        group,
        ping_task(ping_machine, ping_mbox, ping_id),
        ChildPolicy::Restart { max: 1 }
    );
    bloxide_tokio::spawn_child!(
        group,
        pong_task(pong_machine, pong_mbox, pong_id),
        ChildPolicy::Stop
    );
    let sup_control_ref = group.control_ref();
    let sup_notify = group.notify_sender();
    let sup_id = bloxide_tokio::next_actor_id!();
    let (children, sup_notify_rx, sup_control_rx) = group.finish();

    tracing::info!(sup_id, "supervisor setup");

    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));

    let health_ref = sup_control_ref.clone();
    let _health_task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        loop {
            ticker.tick().await;
            if health_ref
                .try_send(sup_id, SupervisorControl::HealthCheckTick)
                .is_err()
            {
                break;
            }
        }
    });

    let ((pong2_ref,), pong2_mbox) = bloxide_tokio::channels! {
        PingPongMsg(16),
    };
    let pong2_id = pong2_ref.id();
    let pong2_ctx = PongCtx::new(pong2_id, ping_ref);
    let pong2_machine = StateMachine::new(pong2_ctx);
    bloxide_tokio::spawn_child_dynamic!(
        sup_id,
        sup_control_ref,
        sup_notify,
        pong_task(pong2_machine, pong2_mbox, pong2_id),
        ChildPolicy::Stop
    )
    .expect("supervisor control channel should accept dynamic registration");

    supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;
}
MAIN

cargo run -p tokio-demo
