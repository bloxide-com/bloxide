#!/bin/bash
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"
cargo build -p cargo-blox --quiet

BLOX="$REPO_ROOT/target/debug/cargo-blox blox"

DEMO="demo/tokio-minimal"
rm -rf "$REPO_ROOT/$DEMO"
mkdir -p "$REPO_ROOT/$DEMO"
cd "$REPO_ROOT/$DEMO"

# ── Minimal workspace (cargo-blox will add members) ──────────────────────────
cat > Cargo.toml <<'EOF'
[workspace]
members = []
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"
license = "MIT"
repository = "https://github.com/bloxide-com/bloxide"
EOF

# ── Layer 1: Messages ────────────────────────────────────────────────────────
$BLOX new-messages counter
$BLOX add-message counter-messages Tick

# ── Layer 2: Actions ──────────────────────────────────────────────────────────
$BLOX new-actions counter

# Write the action function (user-edited file in actions crate)
cat > crates/actions/counter-actions/src/lib.rs <<'ACTIONS_LIB'
// Copyright 2025 Bloxide, all rights reserved
//! Action traits and generic functions for Counter.
#![no_std]

use bloxide_macros::delegatable;

pub mod prelude {
    pub use crate::*;
}

#[delegatable]
pub trait CountsTicks {
    type Count: Copy + PartialOrd + core::ops::Add<Output = Self::Count> + From<u8>;
    fn count(&self) -> Self::Count;
    fn set_count(&mut self, count: Self::Count);
}

pub fn increment_count<T: CountsTicks>(ctx: &mut T) {
    let new_count = ctx.count() + 1.into();
    ctx.set_count(new_count);
}
ACTIONS_LIB

# ── Layer 4: Blox ───────────────────────────────────────────────────────────
$BLOX new counter --messages counter-messages --actions counter-actions
$BLOX add-state counter Ready
$BLOX add-state counter Done --terminal

# ── Generate boilerplate ────────────────────────────────────────────────────
$BLOX generate

# ── Write action functions (only user-edited file) ───────────────────────────
cat > crates/bloxes/counter/src/actions.rs <<'ACTIONS'
// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{spec::StateFns, transition::ActionResult, transitions};
use counter_actions::{increment_count, CountsTicks};
use counter_messages::CounterMsg;
use crate::{CounterCtx, CounterEvent, CounterSpec, CounterState};

impl<B: CountsTicks + 'static> CounterSpec<B> {
    fn count_tick(ctx: &mut CounterCtx<B>, _ev: &CounterEvent) -> ActionResult {
        increment_count(ctx);
        ActionResult::Ok
    }
}

impl<B: CountsTicks + 'static> CounterSpec<B> {
    pub const READY_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            CounterMsg::Tick(_tick) => {
                actions [Self::count_tick]
                guard(_ctx, _results) {
                    _ => CounterState::Done,
                }
            },
        ],
    };

    pub const DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: &[],
    };
}
ACTIONS

# ── Layer 5: Binary ─────────────────────────────────────────────────────────
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
counter-blox      = { workspace = true }
counter-actions   = { workspace = true }
counter-messages  = { workspace = true }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
CRATE

cat > apps/tokio-minimal/src/main.rs <<'MAIN'
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_tokio::prelude::*;
use counter_blox::prelude::*;
use counter_messages::prelude::*;

#[derive(Default)]
struct DemoBehavior {
    count: u8,
}

impl counter_actions::CountsTicks for DemoBehavior {
    type Count = u8;
    fn count(&self) -> u8 { self.count }
    fn set_count(&mut self, count: u8) { self.count = count; }
}

bloxide_tokio::actor_task_supervised!(counter_task, CounterSpec<DemoBehavior>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .try_init()
        .ok();

    let ((counter_ref,), counter_mbox) = bloxide_tokio::channels! {
        CounterMsg(8),
    };
    let counter_id = counter_ref.id();

    let machine = StateMachine::<CounterSpec<DemoBehavior>>::new(
        CounterCtx::new(counter_id, DemoBehavior::default())
    );

    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
    bloxide_tokio::spawn_child!(
        group,
        counter_task(machine, counter_mbox, counter_id),
        ChildPolicy::Stop
    );
    let _sup_control_ref = group.control_ref();
    let _sup_notify = group.notify_sender();
    let (children, sup_notify_rx, sup_control_rx) = group.finish();

    let sup_ctx = SupervisorCtx::new(bloxide_tokio::next_actor_id!(), children);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));

    counter_ref
        .try_send(counter_id, CounterMsg::Tick(counter_messages::Tick))
        .expect("first tick");

    supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;
    println!("tokio-minimal-demo complete");
}
MAIN

# ── Write final workspace Cargo.toml ────────────────────────────────────────
cat > Cargo.toml <<'EOF'
[workspace]
members = [
    "crates/messages/counter-messages",
    "crates/actions/counter-actions",
    "crates/bloxes/counter",
    "apps/tokio-minimal",
]
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"
license = "MIT"
repository = "https://github.com/bloxide-com/bloxide"

[workspace.dependencies]
bloxide-core      = { path = "../../crates/bloxide-core" }
bloxide-tokio     = { path = "../../runtimes/bloxide-tokio" }
bloxide-macros    = { path = "../../crates/bloxide-macros" }
counter-messages  = { path = "crates/messages/counter-messages" }
counter-actions   = { path = "crates/actions/counter-actions" }
counter-blox      = { path = "crates/bloxes/counter" }
tokio             = { version = "1", features = ["full"] }
tracing           = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[profile.dev]
panic = "abort"
EOF

cargo run -p tokio-minimal
