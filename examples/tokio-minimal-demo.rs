// Copyright 2025 Bloxide, all rights reserved
//! Minimal Tokio demo — scaffolds a counter actor via `cargo-blox`, writes
//! the user-edited action functions, generates boilerplate, and runs the
//! resulting binary.

mod common;

use common::*;

fn main() {
    let root = repo_root();
    let blox_bin = ensure_cargo_blox(&root);
    let demo = create_demo_dir(&root, "tokio-minimal");

    // ── Minimal workspace ─────────────────────────────────────────────────────
    write_file(&demo, "Cargo.toml", r#"[workspace]
members = []
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"
license = "MIT"
repository = "https://github.com/bloxide-com/bloxide"
"#);

    // ── Layer 1: Messages ─────────────────────────────────────────────────────
    blox(&blox_bin, &demo, &["new-messages", "counter"]);
    blox(&blox_bin, &demo, &["add-message", "counter-messages", "Tick"]);

    // ── Layer 2: Actions ──────────────────────────────────────────────────────
    blox(&blox_bin, &demo, &["new-actions", "counter"]);

    write_file(&demo, "crates/actions/counter-actions/src/lib.rs", r#"// Copyright 2025 Bloxide, all rights reserved
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
"#);

    // ── Layer 4: Blox ─────────────────────────────────────────────────────────
    blox(&blox_bin, &demo, &["new", "counter", "--messages", "counter-messages", "--actions", "counter-actions"]);
    blox(&blox_bin, &demo, &["add-state", "counter", "Ready"]);
    blox(&blox_bin, &demo, &["add-state", "counter", "Done", "--terminal"]);

    // ── Generate boilerplate ──────────────────────────────────────────────────
    blox(&blox_bin, &demo, &["generate"]);

    // ── Write action functions (user-edited file) ─────────────────────────────
    write_file(&demo, "crates/bloxes/counter/src/actions.rs", r#"// Copyright 2025 Bloxide, all rights reserved
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
"#);

    // ── Layer 5: Binary ───────────────────────────────────────────────────────
    write_file(&demo, "apps/tokio-minimal/Cargo.toml", r#"[package]
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
"#);

    write_file(&demo, "apps/tokio-minimal/src/main.rs", r#"use bloxide_core::lifecycle::LifecycleCommand;
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
"#);

    // ── Final workspace Cargo.toml ────────────────────────────────────────────
    write_file(&demo, "Cargo.toml", r#"[workspace]
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
"#);

    println!("=== Setup complete. Running demo... ===");
    cargo_run(&demo, "tokio-minimal");
}
