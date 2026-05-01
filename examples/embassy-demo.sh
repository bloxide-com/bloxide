#!/bin/bash
set -e

DEMO="demo/embassy"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
rm -rf "$REPO_ROOT/$DEMO"
mkdir -p "$REPO_ROOT/$DEMO"

cd "$REPO_ROOT/$DEMO"

cat > Cargo.toml <<'WORKSPACE'
[workspace]
members = ["apps/embassy-demo"]
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"

[workspace.dependencies]
bloxide-core         = { path = "../../../crates/bloxide-core" }
bloxide-embassy      = { path = "../../../runtimes/bloxide-embassy", features = ["std"] }
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
mkdir -p apps/embassy-demo/src

cat > apps/embassy-demo/Cargo.toml <<'CRATE'
[package]
name = "embassy-demo"
version.workspace = true
edition.workspace = true
publish = false

[dependencies]
bloxide-core       = { workspace = true, features = ["std"] }
bloxide-embassy    = { workspace = true }
bloxide-log        = { workspace = true }
bloxide-timer      = { workspace = true, features = ["std"] }
ping-blox          = { workspace = true }
pong-blox          = { workspace = true }
ping-pong-messages = { workspace = true }
embassy-demo-impl  = { workspace = true }
embassy-executor   = { version = "0.9", features = ["arch-std", "executor-thread"] }
embassy-sync       = { version = "0.7" }
embassy-time       = { version = "0.5", features = ["std", "generic-queue-8"] }
critical-section   = { version = "1.2", features = ["std"] }
static_cell        = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-log = "0.2"
CRATE

cat > apps/embassy-demo/src/main.rs <<'MAIN'
extern crate alloc;

use bloxide_embassy::prelude::*;

use bloxide_core::lifecycle::LifecycleCommand;
use embassy_demo_impl::PingBehavior;
use ping_blox::prelude::*;
use ping_pong_messages::prelude::*;
use pong_blox::prelude::*;

bloxide_embassy::timer_task!(timer_task);
bloxide_embassy::root_task!(
    supervisor_task,
    SupervisorSpec<EmbassyRuntime>,
    std::process::exit(0)
);
bloxide_embassy::actor_task_supervised!(ping_task, PingSpec<EmbassyRuntime, PingBehavior>);
bloxide_embassy::actor_task_supervised!(pong_task, PongSpec<EmbassyRuntime>);

static EXECUTOR: static_cell::StaticCell<embassy_executor::Executor> =
    static_cell::StaticCell::new();

fn main() {
    tracing_log::LogTracer::init().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("trace")),
        )
        .try_init()
        .ok();

    let executor = EXECUTOR.init(embassy_executor::Executor::new());
    executor.run(setup);
}

fn setup(spawner: Spawner) {
    let timer_ref = bloxide_embassy::spawn_timer!(spawner, timer_task, 8);

    let ((ping_ref,), ping_mbox) = bloxide_embassy::channels! {
        PingPongMsg(16),
    };
    let ping_id = ping_ref.id();

    let ((pong_ref,), pong_mbox) = bloxide_embassy::channels! {
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
    let pong_ctx = PongCtx::new(pong_id, ping_ref);

    let ping_machine = StateMachine::new(ping_ctx);
    let pong_machine = StateMachine::new(pong_ctx);

    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
    bloxide_embassy::spawn_child!(
        spawner,
        group,
        ping_task(ping_machine, ping_mbox, ping_id),
        ChildPolicy::Restart { max: 1 }
    );
    bloxide_embassy::spawn_child!(
        spawner,
        group,
        pong_task(pong_machine, pong_mbox, pong_id),
        ChildPolicy::Stop
    );
    let _sup_control_ref = group.control_ref();
    let sup_id = bloxide_embassy::next_actor_id!();
    let (children, sup_notify_rx, sup_control_rx) = group.finish();

    tracing::info!(sup_id, "supervisor setup");

    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::new(sup_ctx);
    sup_machine.dispatch(SupervisorEvent::<EmbassyRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));

    spawner.must_spawn(supervisor_task(
        sup_machine,
        (sup_notify_rx, sup_control_rx),
    ));
}
MAIN

cargo run -p embassy-demo
