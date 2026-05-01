#!/bin/bash
set -e

echo "=== BhsmTst Interactive Demo Setup ==="

DEMO="demo/bhsm-tst"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
rm -rf "$REPO_ROOT/$DEMO"
mkdir -p "$REPO_ROOT/$DEMO"

cd "$REPO_ROOT/$DEMO"

# ── Workspace manifest ──────────────────────────────────────────────────────
cat > Cargo.toml <<'WORKSPACE'
[workspace]
members = ["apps/bhsm-tst-demo"]
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"

[workspace.dependencies]
bloxide-core        = { path = "../../../crates/bloxide-core" }
bloxide-tokio       = { path = "../../../runtimes/bloxide-tokio" }
bloxide-macros      = { path = "../../../crates/bloxide-macros" }
bloxide-log         = { path = "../../../crates/bloxide-log", features = ["log"] }
bhsm-tst-messages   = { path = "../../../crates/messages/bhsm-tst-messages" }
bhsm-tst-actions    = { path = "../../../crates/actions/bhsm-tst-actions" }
bhsm-tst-blox       = { path = "../../../crates/bloxes/bhsm-tst" }

[profile.dev]
panic = "abort"
WORKSPACE

# ── Binary app crate ────────────────────────────────────────────────────────
mkdir -p apps/bhsm-tst-demo/src

cat > apps/bhsm-tst-demo/Cargo.toml <<'CRATE'
[package]
name = "bhsm-tst-demo"
version.workspace = true
edition.workspace = true
publish = false

[dependencies]
bloxide-core      = { workspace = true, features = ["std"] }
bloxide-tokio     = { workspace = true }
bloxide-log       = { workspace = true }
bhsm-tst-blox     = { workspace = true }
bhsm-tst-messages = { workspace = true }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-log = "0.2"
CRATE

cat > apps/bhsm-tst-demo/src/main.rs <<'MAIN'
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_tokio::prelude::*;
use bhsm_tst_blox::prelude::*;
use bhsm_tst_messages::prelude::*;
use std::time::Duration;
use tokio::io::{self, AsyncBufReadExt};

bloxide_tokio::actor_task_supervised!(bhsm_task, BhsmTstSpec<TokioRuntime>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

fn print_usage() {
    eprintln!("BhsmTst commands:");
    eprintln!("  A B C D E F G H I K X  — send BhsmTstMsg variant to actor");
    eprintln!("  K  — trigger error (supervisor restarts actor)");
    eprintln!("  X  — terminal Done (supervisor shuts down)");
    eprintln!("  ?  — print this help");
}

#[tokio::main]
async fn main() {
    tracing_log::LogTracer::init().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init()
        .ok();

    let ((bhsm_ref,), bhsm_mbox) = bloxide_tokio::channels! {
        BhsmTstMsg(16),
    };
    let bhsm_id = bhsm_ref.id();

    tracing::info!(bhsm_id, "setting up BhsmTst actor");

    let bhsm_ctx = BhsmTstCtx::new(bloxide_tokio::next_actor_id!());
    let bhsm_machine = StateMachine::new(bhsm_ctx);

    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
    bloxide_tokio::spawn_child!(
        group,
        bhsm_task(bhsm_machine, bhsm_mbox, bhsm_id),
        ChildPolicy::Restart { max: 3 }
    );
    let sup_control_ref = group.control_ref();
    let _sup_notify = group.notify_sender();
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

    let stdin_ref = bhsm_ref.clone();
    let _stdin_task = tokio::spawn(async move {
        let stdin = io::BufReader::new(io::stdin());
        let mut lines = stdin.lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let cmd = line.trim().to_uppercase();
                    let ch = cmd.chars().next();
                    match ch {
                        Some('A') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::A(A)); }
                        Some('B') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::B(B)); }
                        Some('C') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::C(C)); }
                        Some('D') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::D(D)); }
                        Some('E') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::E(E)); }
                        Some('F') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::F(F)); }
                        Some('G') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::G(G)); }
                        Some('H') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::H(H)); }
                        Some('I') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::I(I)); }
                        Some('K') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::K(K)); }
                        Some('X') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::X(X)); }
                        Some('?') => print_usage(),
                        _ => eprintln!("unknown command: {:?}", cmd),
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
    });

    print_usage();
    supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;
}
MAIN

echo "=== Setup complete. Running demo... ==="
cargo run -p bhsm-tst-demo
