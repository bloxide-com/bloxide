#!/bin/bash
set -e

# Build the cargo-blox tool first
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"
cargo build -p cargo-blox --quiet

BLOX="cargo run -p cargo-blox --quiet -- blox"

DEMO="demo/embassy"
rm -rf "$REPO_ROOT/$DEMO"
mkdir -p "$REPO_ROOT/$DEMO"
cd "$REPO_ROOT/$DEMO"

# ── Workspace Cargo.toml ────────────────────────────────────────────────────
cat > Cargo.toml <<'WORKSPACE'
[workspace]
members = [
    "crates/messages/ping-pong-messages",
    "crates/actions/ping-pong-actions",
    "crates/bloxes/ping",
    "crates/bloxes/pong",
    "apps/embassy-demo",
]
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
ping-pong-messages   = { path = "crates/messages/ping-pong-messages" }
ping-pong-actions    = { path = "crates/actions/ping-pong-actions" }
ping-blox            = { path = "crates/bloxes/ping" }
pong-blox            = { path = "crates/bloxes/pong" }

[profile.dev]
panic = "abort"
WORKSPACE

# ── Layer 1: Messages ─────────────────────────────────────────────────────
$BLOX new-messages ping-pong
$BLOX add-message ping-pong-messages Ping round:u32
$BLOX add-message ping-pong-messages Pong round:u32
$BLOX add-message ping-pong-messages Resume

# ── Layer 2: Actions ────────────────────────────────────────────────────────
$BLOX new-actions ping-pong

# ── Layer 4: Blox ─────────────────────────────────────────────────────────
$BLOX new ping --messages ping-pong-messages --actions ping-pong-actions
$BLOX add-state ping Operating --composite
$BLOX add-state ping Active --parent Operating
$BLOX add-state ping Paused --parent Operating
$BLOX add-state ping Done
$BLOX add-state ping Error

$BLOX new pong --messages ping-pong-messages --actions ping-pong-actions
$BLOX add-state pong Ready

# Generate boilerplate from TOML
$BLOX generate

# ── Write action crate (user-edited) ─────────────────────────────────────────
cat > crates/actions/ping-pong-actions/src/lib.rs <<'ACTIONS'
// Copyright 2025 Bloxide, all rights reserved
#![no_std]

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
    type Round: Copy
        + PartialEq
        + PartialOrd
        + core::ops::Add<Output = Self::Round>
        + From<u8>
        + core::fmt::Display;
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

pub fn schedule_resume<R, C>(ctx: &mut C, duration_ms: u64)
where
    R: BloxRuntime,
    C: HasSelfRef<R> + HasTimerRef<R> + HasSelfId + HasCurrentTimer,
{
    let id = set_timer::<R, C, PingPongMsg>(
        ctx,
        duration_ms,
        ctx.self_ref(),
        PingPongMsg::Resume(Resume),
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

pub fn send_ping<R, C>(ctx: &mut C) -> ActionResult
where
    R: BloxRuntime,
    C: HasSelfId + HasPeerRef<R> + CountsRounds,
    C::Round: Into<u32>,
{
    ActionResult::from(ctx.peer_ref().try_send(
        ctx.self_id(),
        PingPongMsg::Ping(Ping {
            round: ctx.round().into(),
        }),
    ))
}

pub fn send_initial_ping<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasPeerRef<R> + CountsRounds,
    C::Round: Into<u32>,
{
    if ctx.round() == C::Round::from(1) {
        if ctx
            .peer_ref()
            .try_send(
                ctx.self_id(),
                PingPongMsg::Ping(Ping {
                    round: ctx.round().into(),
                }),
            )
            .is_err()
        {
            bloxide_log::blox_log_warn!(
                ctx.self_id(),
                "send_initial_ping: peer channel full, first Ping dropped"
            );
        }
    }
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
ACTIONS

# ── Write Ping blox actions (user-edited) ──────────────────────────────────
cat > crates/bloxes/ping/src/actions.rs <<'PING_ACTIONS'
// Copyright 2025 Bloxide, all rights reserved
use crate::{PingCtx, PingEvent, PingSpec, MAX_ROUNDS, PAUSE_AT_ROUND, PAUSE_DURATION_MS};
use bloxide_core::{
    capability::BloxRuntime,
    spec::StateFns,
    transition::ActionResult,
    transitions, HasSelfId,
};
use ping_pong_actions::{
    cancel_current_timer, increment_round, schedule_resume, send_initial_ping, send_ping,
    CountsRounds, HasCurrentTimer,
};
use ping_pong_messages::PingPongMsg;

use crate::PingState;

impl<R, B> PingSpec<R, B>
where
    R: BloxRuntime,
    B: HasCurrentTimer + CountsRounds + Default + 'static,
    B::Round: Into<u32>,
{
    fn log_pong_received(ctx: &mut PingCtx<R, B>, ev: &PingEvent) -> ActionResult {
        if let Some(PingPongMsg::Pong(pong)) = ev.msg_payload() {
            bloxide_log::blox_log_debug!(ctx.self_id(), "Pong({}) received", pong.round);
        }
        ActionResult::Ok
    }

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

    fn log_error(ctx: &mut PingCtx<R, B>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "entered error state");
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
                actions [Self::log_pong_received, Self::forward_ping]
                guard(ctx, results) {
                    results.any_failed()                          => PingState::Error,
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
        on_entry: &[Self::log_error],
        on_exit: &[],
        transitions: &[],
    };
}
PING_ACTIONS

# ── Add constants to Ping blox lib.rs ─────────────────────────────────────
cat > crates/bloxes/ping/src/lib.rs <<'PING_LIB'
// Copyright 2025 Bloxide, all rights reserved
#![no_std]

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
PING_LIB

# ── Write Pong blox actions (user-edited) ──────────────────────────────────
cat > crates/bloxes/pong/src/actions.rs <<'PONG_ACTIONS'
// Copyright 2025 Bloxide, all rights reserved
use crate::prelude::*;
use bloxide_core::{capability::BloxRuntime, spec::StateFns, transition::ActionResult, transitions};
use ping_pong_actions::send_pong;
use ping_pong_messages::PingPongMsg;

impl<R: BloxRuntime> PongSpec<R> {
    fn reply_pong_action(ctx: &mut PongCtx<R>, ev: &PongEvent) -> ActionResult {
        if let Some(PingPongMsg::Ping(ping)) = ev.msg_payload() {
            return send_pong::<R, _>(ctx, ping);
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
PONG_ACTIONS

# ── Layer 5: Binary (Embassy-specific) ─────────────────────────────────────
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
ping-pong-actions  = { workspace = true }
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
