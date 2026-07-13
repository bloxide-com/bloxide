// Copyright 2025 Bloxide, all rights reserved
//! Embassy ping-pong demo — uses `cargo-blox` CLI to scaffold, writes
//! user-edited files, patches generated output, and runs an Embassy-based
//! ping/pong actor system.

mod common;

use common::*;
use std::fs;

fn main() {
    let root = repo_root();
    let blox_bin = ensure_cargo_blox(&root);
    let demo = create_demo_dir(&root, "embassy");

    // ── Initial workspace ─────────────────────────────────────────────────────
    write_file(&demo, "Cargo.toml", r#"[workspace]
members = [
]
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"
license = "MIT"
repository = "https://github.com/bloxide-com/bloxide"

[workspace.dependencies]
"#);

    // ── Layer 1: Messages ─────────────────────────────────────────────────────
    blox(&blox_bin, &demo, &["new-messages", "ping-pong"]);
    blox(&blox_bin, &demo, &["add-message", "ping_pong-messages", "Ping", "round:u32"]);
    blox(&blox_bin, &demo, &["add-message", "ping_pong-messages", "Pong", "round:u32"]);
    blox(&blox_bin, &demo, &["add-message", "ping_pong-messages", "Resume"]);

    // ── Layer 2: Actions ──────────────────────────────────────────────────────
    blox(&blox_bin, &demo, &["new-actions", "ping-pong"]);

    // ── Layer 4: Blox — Ping ──────────────────────────────────────────────────
    blox(&blox_bin, &demo, &["new", "ping", "--messages", "ping_pong-messages"]);
    blox(&blox_bin, &demo, &["add-state", "ping", "Operating", "--composite"]);
    blox(&blox_bin, &demo, &["add-state", "ping", "Active", "--parent", "Operating"]);
    blox(&blox_bin, &demo, &["add-state", "ping", "Paused", "--parent", "Operating"]);
    blox(&blox_bin, &demo, &["add-state", "ping", "Done", "--terminal"]);
    blox(&blox_bin, &demo, &["add-state", "ping", "Error", "--error"]);

    // ── Layer 4: Blox — Pong ──────────────────────────────────────────────────
    blox(&blox_bin, &demo, &["new", "pong", "--messages", "ping_pong-messages"]);
    blox(&blox_bin, &demo, &["add-state", "pong", "Ready"]);

    // ── Patch Ping blox.toml: add context and mailboxes ───────────────────────
    patch_ping_blox_toml(&demo);
    patch_pong_blox_toml(&demo);

    // ── Generate all boilerplate from TOML ────────────────────────────────────
    blox(&blox_bin, &demo, &["generate"]);

    // ── Patch generated spec_skeleton.rs and ctx.rs ───────────────────────────
    patch_generated_ping(&demo);
    patch_generated_pong(&demo);

    // ── Patch blox Cargo.toml files ───────────────────────────────────────────
    write_file(&demo, "crates/bloxes/ping/Cargo.toml", r#"[package]
name = "ping-blox"
version.workspace = true
edition.workspace = true
description = "Ping actor blox — runtime-agnostic"
repository.workspace = true
license.workspace = true

[features]
default = ["std"]
std = ["bloxide-core/std", "bloxide-log/log"]

[dependencies]
bloxide-core   = { workspace = true, features = ["alloc"] }
bloxide-macros = { workspace = true }
bloxide-log    = { workspace = true }
bloxide-timer  = { workspace = true, features = ["alloc"] }
ping_pong-messages = { workspace = true }
ping_pong-actions  = { workspace = true }
"#);

    write_file(&demo, "crates/bloxes/pong/Cargo.toml", r#"[package]
name = "pong-blox"
version.workspace = true
edition.workspace = true
description = "Pong actor blox — runtime-agnostic"
repository.workspace = true
license.workspace = true

[features]
default = ["std"]
std = ["bloxide-core/std", "bloxide-log/log"]

[dependencies]
bloxide-core   = { workspace = true, features = ["alloc"] }
bloxide-macros = { workspace = true }
bloxide-log    = { workspace = true }
ping_pong-messages = { workspace = true }
ping_pong-actions  = { workspace = true }
"#);

    // ── Patch actions crate Cargo.toml ────────────────────────────────────────
    write_file(&demo, "crates/actions/ping_pong-actions/Cargo.toml", r#"[package]
name = "ping_pong-actions"
version.workspace = true
edition.workspace = true
description = "Action traits and generic functions for PingPong"
repository.workspace = true
license.workspace = true

[dependencies]
bloxide-core   = { workspace = true, features = ["alloc"] }
bloxide-macros = { workspace = true }
bloxide-timer  = { workspace = true, features = ["alloc"] }
ping_pong-messages = { workspace = true }
"#);

    // ── Write ping-pong-actions (trait + generic functions) ───────────────────
    write_file(&demo, "crates/actions/ping_pong-actions/src/lib.rs", r#"#![no_std]

use bloxide_core::{
    accessor::HasSelfId, capability::BloxRuntime, messaging::ActorRef, transition::ActionResult,
};
use bloxide_macros::delegatable;
use bloxide_timer::{cancel_timer, set_timer, HasTimerRef, TimerId};
use ping_pong_messages::{Ping, PingPongMsg, Pong, Resume};

pub trait HasPeerRef<R: BloxRuntime> {
    fn peer_ref(&self) -> &ActorRef<PingPongMsg, R>;
}

pub trait HasSelfRef<R: BloxRuntime> {
    fn self_ref(&self) -> &ActorRef<PingPongMsg, R>;
}

#[delegatable]
pub trait CountsRounds {
    type Round: Copy + PartialEq + PartialOrd
        + core::ops::Add<Output = Self::Round>
        + From<u8> + core::fmt::Display;
    fn round(&self) -> Self::Round;
    fn set_round(&mut self, round: Self::Round);
}

#[delegatable]
pub trait HasCurrentTimer {
    fn current_timer(&self) -> Option<TimerId>;
    fn set_current_timer(&mut self, timer: Option<TimerId>);
}

pub fn increment_round<C: CountsRounds>(ctx: &mut C) {
    let one = C::Round::from(1);
    ctx.set_round(ctx.round() + one);
}

pub fn send_initial_ping<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasPeerRef<R> + CountsRounds,
    C::Round: Into<u32>,
{
    if ctx.round() == C::Round::from(1) {
        let _ = ctx.peer_ref().try_send(
            ctx.self_id(),
            PingPongMsg::Ping(Ping { round: ctx.round().into() }),
        );
    }
}

pub fn send_ping<R, C>(ctx: &mut C) -> ActionResult
where
    R: BloxRuntime,
    C: HasSelfId + HasPeerRef<R> + CountsRounds,
    C::Round: Into<u32>,
{
    ActionResult::from(ctx.peer_ref().try_send(
        ctx.self_id(),
        PingPongMsg::Ping(Ping { round: ctx.round().into() }),
    ))
}

pub fn send_pong<R, C>(ctx: &mut C, ping: &Ping) -> ActionResult
where
    R: BloxRuntime,
    C: HasSelfId + HasPeerRef<R>,
{
    ActionResult::from(
        ctx.peer_ref()
            .try_send(ctx.self_id(), PingPongMsg::Pong(Pong { round: ping.round })),
    )
}

pub fn schedule_resume<R, C>(ctx: &mut C, duration_ms: u64)
where
    R: BloxRuntime,
    C: HasSelfRef<R> + HasTimerRef<R> + HasSelfId + HasCurrentTimer,
{
    let id = set_timer::<R, C, PingPongMsg>(
        ctx, duration_ms, ctx.self_ref(), PingPongMsg::Resume(Resume),
    );
    ctx.set_current_timer(Some(id));
}

pub fn cancel_current_timer<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasTimerRef<R> + HasCurrentTimer,
{
    if let Some(id) = ctx.current_timer() {
        cancel_timer::<R, C>(ctx, id);
        ctx.set_current_timer(None);
    }
}
"#);

    // ── Write Ping actions ────────────────────────────────────────────────────
    write_file(&demo, "crates/bloxes/ping/src/actions.rs", r#"use crate::{PingCtx, PingEvent, PingSpec, PingState, MAX_ROUNDS, PAUSE_AT_ROUND, PAUSE_DURATION_MS};
use bloxide_core::{
    capability::BloxRuntime, spec::StateFns, transition::ActionResult, transitions, HasSelfId,
};
use ping_pong_actions::{
    cancel_current_timer, increment_round, schedule_resume, send_initial_ping, send_ping,
    CountsRounds, HasCurrentTimer,
};
use ping_pong_messages::PingPongMsg;

impl<R, B> PingSpec<R, B>
where
    R: BloxRuntime,
    B: HasCurrentTimer + CountsRounds + Default + 'static,
    B::Round: Into<u32>,
{
    fn forward_ping(ctx: &mut PingCtx<R, B>, _ev: &PingEvent) -> ActionResult {
        send_ping::<R, _>(ctx)
    }

    fn schedule_pause_timer(ctx: &mut PingCtx<R, B>) {
        schedule_resume::<R, _>(ctx, PAUSE_DURATION_MS);
        bloxide_log::blox_log_info!(
            ctx.self_id(),
            "paused — resuming in {}ms",
            PAUSE_DURATION_MS
        );
    }

    fn cancel_pause_timer(ctx: &mut PingCtx<R, B>) {
        cancel_current_timer::<R, _>(ctx);
    }

    fn log_round(ctx: &mut PingCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "round {} — sending Ping", ctx.round());
    }

    fn log_done(ctx: &mut PingCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "done after {} rounds", ctx.round());
    }

    pub(crate) const OPERATING_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PingPongMsg::Pong(_) => stay,
        ],
    };

    pub(crate) const ACTIVE_FNS: StateFns<Self> = StateFns {
        on_entry: &[increment_round, Self::log_round, send_initial_ping],
        on_exit: &[],
        transitions: transitions![
            PingPongMsg::Pong(_) => {
                actions [Self::forward_ping]
                guard(ctx, _results) {
                    ctx.round() >= B::Round::from(MAX_ROUNDS)     => PingState::Done,
                    ctx.round() == B::Round::from(PAUSE_AT_ROUND) => PingState::Paused,
                    _                                             => PingState::Active,
                }
            },
        ],
    };

    pub(crate) const PAUSED_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::schedule_pause_timer],
        on_exit: &[Self::cancel_pause_timer],
        transitions: transitions![
            PingPongMsg::Resume(_resume) => {
                actions [Self::forward_ping]
                transition PingState::Active
            },
        ],
    };

    pub(crate) const DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::log_done],
        on_exit: &[],
        transitions: &[],
    };

    pub(crate) const ERROR_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: &[],
    };
}
"#);

    // ── Write Ping lib.rs ─────────────────────────────────────────────────────
    write_file(&demo, "crates/bloxes/ping/src/lib.rs", r#"#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod generated;
pub mod actions;
pub mod prelude;

#[cfg(all(test, feature = "std"))]
mod tests;

pub use generated::*;

pub const MAX_ROUNDS: u8 = 5;
pub const PAUSE_AT_ROUND: u8 = 2;
pub const PAUSE_DURATION_MS: u64 = 150;
"#);

    // ── Write Pong actions ────────────────────────────────────────────────────
    write_file(&demo, "crates/bloxes/pong/src/actions.rs", r#"use crate::prelude::*;
use bloxide_core::{capability::BloxRuntime, spec::StateFns, transition::ActionResult, transitions};
use ping_pong_actions::send_pong;
use ping_pong_messages::PingPongMsg;

impl<R: BloxRuntime> PongSpec<R> {
    fn reply_pong_action(ctx: &mut PongCtx<R>, ev: &PongEvent) -> ActionResult {
        if let Some(PingPongMsg::Ping(ping)) = ev.msg_payload() {
            return send_pong::<R, _>(ctx, &ping);
        }
        ActionResult::Ok
    }

    pub const READY_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PingPongMsg::Ping(_) => {
                actions [Self::reply_pong_action]
                stay
            },
        ],
    };
}
"#);

    // ── Rewrite workspace Cargo.toml with full deps + binary ──────────────────
    write_file(&demo, "Cargo.toml", r#"[workspace]
members = [
    "crates/messages/ping_pong-messages",
    "crates/actions/ping_pong-actions",
    "crates/bloxes/ping",
    "crates/bloxes/pong",
    "apps/embassy-demo",
]
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"
license = "MIT"
repository = "https://github.com/bloxide-com/bloxide"

[workspace.dependencies]
bloxide-core         = { path = "../../crates/bloxide-core" }
bloxide-embassy      = { path = "../../runtimes/bloxide-embassy", features = ["std"] }
bloxide-macros       = { path = "../../crates/bloxide-macros" }
bloxide-log          = { path = "../../crates/bloxide-log", features = ["log"] }
bloxide-timer        = { path = "../../crates/bloxide-timer" }
ping_pong-messages   = { path = "crates/messages/ping_pong-messages" }
ping_pong-actions    = { path = "crates/actions/ping_pong-actions" }
ping-blox            = { path = "crates/bloxes/ping" }
pong-blox            = { path = "crates/bloxes/pong" }
embassy-executor     = { version = "0.9", features = ["arch-std", "executor-thread"] }
embassy-sync        = { version = "0.7" }
embassy-time        = { version = "0.5", features = ["std", "generic-queue-8"] }
critical-section    = { version = "1.2", features = ["std"] }
static_cell         = "2"
log                 = "0.4"
env_logger          = "0.11"

[profile.dev]
panic = "abort"
"#);

    // ── Layer 5: Binary (Embassy) ─────────────────────────────────────────────
    write_file(&demo, "apps/embassy-demo/Cargo.toml", r#"[package]
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
ping_pong-messages = { workspace = true }
ping_pong-actions  = { workspace = true }
embassy-executor   = { workspace = true }
embassy-sync       = { workspace = true }
embassy-time       = { workspace = true }
critical-section   = { workspace = true }
static_cell        = { workspace = true }
log                = { workspace = true }
env_logger         = { workspace = true }
"#);

    write_file(&demo, "apps/embassy-demo/src/main.rs", r#"use bloxide_embassy::prelude::*;
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_timer::TimerId;
use ping_blox::prelude::*;
use ping_pong_actions::{CountsRounds, HasCurrentTimer};
use ping_pong_messages::prelude::*;
use pong_blox::prelude::*;

#[derive(Debug, Default, Clone)]
struct DemoBehavior {
    round: u32,
    current_timer: Option<TimerId>,
}

impl CountsRounds for DemoBehavior {
    type Round = u32;
    fn round(&self) -> u32 { self.round }
    fn set_round(&mut self, round: u32) { self.round = round; }
}

impl HasCurrentTimer for DemoBehavior {
    fn current_timer(&self) -> Option<TimerId> { self.current_timer }
    fn set_current_timer(&mut self, timer: Option<TimerId>) { self.current_timer = timer; }
}

bloxide_embassy::timer_task!(timer_task);
bloxide_embassy::root_task!(
    supervisor_task,
    SupervisorSpec<EmbassyRuntime>,
    std::process::exit(0)
);
bloxide_embassy::actor_task_supervised!(ping_task, PingSpec<EmbassyRuntime, DemoBehavior>);
bloxide_embassy::actor_task_supervised!(pong_task, PongSpec<EmbassyRuntime>);

static EXECUTOR: static_cell::StaticCell<embassy_executor::Executor> =
    static_cell::StaticCell::new();

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

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

    log::info!("setup: ping_id={} pong_id={}", ping_id, pong_id);

    let ping_ctx = PingCtx::new(
        ping_id,
        pong_ref.clone(),
        ping_ref.clone(),
        timer_ref,
        DemoBehavior::default(),
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

    log::info!("supervisor setup: sup_id={}", sup_id);

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
"#);

    println!("=== Setup complete. Running demo... ===");
    cargo_run(&demo, "embassy-demo");
}

// ── Helpers for patching blox.toml ────────────────────────────────────────────

fn patch_ping_blox_toml(demo: &std::path::Path) {
    let path = demo.join("crates/bloxes/ping/blox.toml");
    let mut txt = fs::read_to_string(&path).expect("read ping blox.toml");
    // Remove default [context] section
    if let Some(pos) = txt.find("[context]") {
        let end = txt[pos..].find("\n[").map(|e| pos + e).unwrap_or(txt.len());
        txt.replace_range(pos..end, "");
    }
    // Remove default [[event.mailboxes]]
    if let Some(pos) = txt.find("[[event.mailboxes]]") {
        let end = txt[pos..].find("\n[").map(|e| pos + e).unwrap_or(txt.len());
        txt.replace_range(pos..end, "");
    }
    // Add initial = true to Active state
    txt = txt.replace(
        "[[topology.states]]\nname = \"Active\"\nparent = \"Operating\"",
        "[[topology.states]]\nname = \"Active\"\nparent = \"Operating\"\ninitial = true",
    );

    txt.push_str(r#"
[context]
name = "PingCtx"
generics = "<R: BloxRuntime, B: HasCurrentTimer + CountsRounds>"
actions_crate = "ping_pong_actions"

[[context.fields]]
name = "self_id"
ty = "ActorId"

[[context.fields]]
name = "peer_ref"
ty = "ActorRef<PingPongMsg, R>"

[[context.fields]]
name = "self_ref"
ty = "ActorRef<PingPongMsg, R>"

[[context.fields]]
name = "timer_ref"
ty = "ActorRef<TimerCommand, R>"

[[context.fields]]
name = "behavior"
ty = "B"
delegates = ["HasCurrentTimer", "CountsRounds"]

[[event.mailboxes]]
variant = "Msg"
message = "PingPongMsg"
message_path = "ping_pong_messages::PingPongMsg"
"#);
    fs::write(&path, txt).expect("write ping blox.toml");
}

fn patch_pong_blox_toml(demo: &std::path::Path) {
    let path = demo.join("crates/bloxes/pong/blox.toml");
    let mut txt = fs::read_to_string(&path).expect("read pong blox.toml");
    // Remove default [context] section
    if let Some(pos) = txt.find("[context]") {
        let end = txt[pos..].find("\n[").map(|e| pos + e).unwrap_or(txt.len());
        txt.replace_range(pos..end, "");
    }
    // Remove default [[event.mailboxes]]
    if let Some(pos) = txt.find("[[event.mailboxes]]") {
        let end = txt[pos..].find("\n[").map(|e| pos + e).unwrap_or(txt.len());
        txt.replace_range(pos..end, "");
    }
    // Add initial = true to Ready state
    txt = txt.replace(
        "[[topology.states]]\nname = \"Ready\"",
        "[[topology.states]]\nname = \"Ready\"\ninitial = true",
    );

    txt.push_str(r#"
[context]
name = "PongCtx"
generics = "<R: BloxRuntime>"
actions_crate = "ping_pong_actions"

[[context.fields]]
name = "self_id"
ty = "ActorId"

[[context.fields]]
name = "peer_ref"
ty = "ActorRef<PingPongMsg, R>"

[[event.mailboxes]]
variant = "Msg"
message = "PingPongMsg"
message_path = "ping_pong_messages::PingPongMsg"
"#);
    fs::write(&path, txt).expect("write pong blox.toml");
}

fn patch_generated_ping(demo: &std::path::Path) {
    write_file(demo, "crates/bloxes/ping/src/generated/spec_skeleton.rs", r#"// Auto-generated by bloxide-codegen. Do not edit manually.
use core::marker::PhantomData;
use bloxide_core::{
    capability::BloxRuntime,
    spec::{MachineSpec, StateFns},
    HasSelfId,
};
use ping_pong_actions::{HasCurrentTimer, CountsRounds};
use ping_pong_messages::PingPongMsg;

use crate::{PingCtx, PingEvent};
pub use crate::generated::topology::PingState;

pub struct PingSpec<R: BloxRuntime, B: HasCurrentTimer + CountsRounds>(PhantomData<(R, B)>);

impl<R, B> MachineSpec for PingSpec<R, B>
where
    R: BloxRuntime,
    B: HasCurrentTimer + CountsRounds + Default + 'static,
    B::Round: Into<u32>,
{
    type State = PingState;
    type Event = PingEvent;
    type Ctx = PingCtx<R, B>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<PingPongMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = ping_state_handler_table!(Self);

    fn initial_state() -> PingState {
        PingState::Active
    }

    fn is_terminal(state: &PingState) -> bool {
        matches!(state, PingState::Done)
    }

    fn is_error(state: &PingState) -> bool {
        matches!(state, PingState::Error)
    }

    fn on_init_entry(ctx: &mut PingCtx<R, B>) {
        ctx.behavior = B::default();
        bloxide_log::blox_log_info!(ctx.self_id(), "reset — behavior cleared");
    }
}
"#);

    write_file(demo, "crates/bloxes/ping/src/generated/ctx.rs", r#"// Auto-generated by bloxide-codegen. Do not edit manually.
use bloxide_core::{ActorId, capability::BloxRuntime, messaging::ActorRef};
use bloxide_macros::BloxCtx;
use bloxide_timer::{HasTimerRef, TimerCommand, TimerId};
use ping_pong_actions::{
    CountsRounds, HasCurrentTimer, HasPeerRef, HasSelfRef,
    __delegate_CountsRounds, __delegate_HasCurrentTimer,
};
use ping_pong_messages::PingPongMsg;

#[derive(BloxCtx)]
pub struct PingCtx<R: BloxRuntime, B: HasCurrentTimer + CountsRounds> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
"#);
}

fn patch_generated_pong(demo: &std::path::Path) {
    write_file(demo, "crates/bloxes/pong/src/generated/spec_skeleton.rs", r#"// Auto-generated by bloxide-codegen. Do not edit manually.
use core::marker::PhantomData;
use bloxide_core::{
    capability::BloxRuntime,
    spec::{MachineSpec, StateFns},
};
use ping_pong_messages::PingPongMsg;

use crate::{PongCtx, PongEvent};
pub use crate::generated::topology::PongState;

pub struct PongSpec<R: BloxRuntime>(PhantomData<R>);

impl<R: BloxRuntime> MachineSpec for PongSpec<R> {
    type State = PongState;
    type Event = PongEvent;
    type Ctx = PongCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<PingPongMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = pong_state_handler_table!(Self);

    fn initial_state() -> PongState {
        PongState::Ready
    }

    fn on_init_entry(_ctx: &mut PongCtx<R>) {}
}
"#);

    write_file(demo, "crates/bloxes/pong/src/generated/ctx.rs", r#"// Auto-generated by bloxide-codegen. Do not edit manually.
use bloxide_core::{capability::BloxRuntime, ActorId, ActorRef};
use bloxide_macros::BloxCtx;
use ping_pong_actions::HasPeerRef;
use ping_pong_messages::PingPongMsg;

#[derive(BloxCtx)]
pub struct PongCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
}
"#);
}
