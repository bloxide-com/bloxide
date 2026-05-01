#!/bin/bash
set -e

DEMO="demo/tokio-pool"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
rm -rf "$REPO_ROOT/$DEMO"
mkdir -p "$REPO_ROOT/$DEMO"

cd "$REPO_ROOT/$DEMO"

cat > Cargo.toml <<'WORKSPACE'
[workspace]
members = ["apps/tokio-pool"]
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"

[workspace.dependencies]
bloxide-core           = { path = "../../../crates/bloxide-core" }
bloxide-tokio          = { path = "../../../runtimes/bloxide-tokio" }
bloxide-macros         = { path = "../../../crates/bloxide-macros" }
bloxide-log            = { path = "../../../crates/bloxide-log", features = ["log"] }
bloxide-spawn          = { path = "../../../crates/bloxide-spawn" }
bloxide-supervisor     = { path = "../../../crates/bloxide-supervisor" }
pool-messages          = { path = "../../../crates/messages/pool-messages" }
pool-actions           = { path = "../../../crates/actions/pool-actions" }
worker-blox            = { path = "../../../crates/bloxes/worker" }
pool-blox              = { path = "../../../crates/bloxes/pool" }
tokio-pool-demo-impl   = { path = "../../../crates/impl/tokio-pool-demo-impl" }

[profile.dev]
panic = "abort"
WORKSPACE

# ── Binary app crate ────────────────────────────────────────────────────────
mkdir -p apps/tokio-pool/src

cat > apps/tokio-pool/Cargo.toml <<'CRATE'
[package]
name = "tokio-pool"
version.workspace = true
edition.workspace = true
publish = false

[dependencies]
bloxide-core       = { workspace = true, features = ["std"] }
bloxide-tokio      = { workspace = true }
bloxide-log        = { workspace = true }
bloxide-spawn      = { workspace = true, features = ["std"] }
bloxide-supervisor = { workspace = true, features = ["std"] }
pool-blox          = { workspace = true, features = ["std"] }
pool-messages      = { workspace = true }
tokio-pool-demo-impl = { workspace = true }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-log = "0.2"
CRATE

cat > apps/tokio-pool/src/main.rs <<'MAIN'
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_tokio::prelude::*;
use pool_blox::{PoolCtx, PoolSpec};
use pool_messages::{PoolMsg, SpawnWorker};
use std::sync::Arc;
use tokio_pool_demo_impl::spawn_worker_tokio;
use tracing_log::LogTracer;

bloxide_tokio::actor_task_supervised!(pool_task, PoolSpec<TokioRuntime>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

#[tokio::main]
async fn main() {
    LogTracer::init().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init()
        .ok();

    let kill_cap = Arc::new(bloxide_tokio::TokioKillCap::new());

    let ((pool_ref,), pool_mbox) = bloxide_tokio::channels! { PoolMsg(32) };
    let pool_id = pool_ref.id();

    tracing::info!(pool_id, "pool created");

    let pool_ctx = PoolCtx::new(pool_id, pool_ref.clone(), spawn_worker_tokio);
    let pool_machine = StateMachine::<PoolSpec<TokioRuntime>>::new(pool_ctx);

    let mut group = ChildGroupBuilder::with_kill_cap(GroupShutdown::WhenAnyDone, kill_cap.clone());
    bloxide_tokio::spawn_child!(
        group,
        pool_task(pool_machine, pool_mbox, pool_id),
        ChildPolicy::Stop
    );
    let _sup_control_ref = group.control_ref();
    let _sup_notify = group.notify_sender();
    let sup_id = bloxide_tokio::next_actor_id!();
    let (children, sup_notify_rx, sup_control_rx) = group.finish();

    tracing::info!(sup_id, pool_id, "supervisor setup complete");

    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));

    let worker_count = 3u32;
    for task_id in 0..worker_count {
        pool_ref
            .try_send(pool_id, PoolMsg::SpawnWorker(SpawnWorker { task_id }))
            .expect("pool mailbox not full");
    }
    tracing::info!(worker_count, "SpawnWorker messages queued");

    supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;

    tracing::info!("pool demo complete");
}
MAIN

cargo run -p tokio-pool
