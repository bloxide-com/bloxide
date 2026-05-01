#!/bin/bash
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"
cargo build -p cargo-blox --quiet
BLOX="cargo run -p cargo-blox --quiet -- blox"

DEMO="demo/tokio-demo"
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
    "apps/tokio-demo",
]
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
ping-pong-messages   = { path = "crates/messages/ping-pong-messages" }
ping-pong-actions    = { path = "crates/actions/ping-pong-actions" }
ping-blox            = { path = "crates/bloxes/ping" }
pong-blox            = { path = "crates/bloxes/pong" }

[profile.dev]
panic = "abort"
WORKSPACE

# ── Layer 1: Messages ────────────────────────────────────────────────────────
$BLOX new-messages ping-pong
$BLOX add-message ping-pong-messages Ping round:u32
$BLOX add-message ping-pong-messages Pong round:u32
$BLOX add-message ping-pong-messages Resume

# ── Layer 2: Actions ────────────────────────────────────────────────────────
$BLOX new-actions ping-pong

# ── Layer 4: Blox — Ping ───────────────────────────────────────────────────
$BLOX new ping --messages ping-pong-messages --actions ping-pong-actions
$BLOX add-state ping Operating --composite
$BLOX add-state ping Active --parent Operating
$BLOX add-state ping Paused --parent Operating
$BLOX add-state ping Done
$BLOX add-state ping Error

# ── Layer 4: Blox — Pong ───────────────────────────────────────────────────
$BLOX new pong --messages ping-pong-messages --actions ping-pong-actions
$BLOX add-state pong Ready

# ── Generate all boilerplate from TOML ───────────────────────────────────────
$BLOX generate

# ── Write ping-pong-actions (trait + generic functions) ──────────────────────
cat > crates/actions/ping-pong-actions/src/lib.rs <<'ACTIONS'
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
ACTIONS

# ── Write Ping actions (simplified) ──────────────────────────────────────────
cat > crates/bloxes/ping/src/actions.rs <<'PING_ACTIONS'
use crate::{PingCtx, PingEvent, PingSpec, PingState, MAX_ROUNDS, PAUSE_AT_ROUND, PAUSE_DURATION_MS};
use bloxide_core::{
    capability::BloxRuntime, spec::StateFns, transition::ActionResult, transitions,
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
    }

    fn cancel_pause_timer(ctx: &mut PingCtx<R, B>) {
        cancel_current_timer::<R, _>(ctx);
    }

    pub(crate) const OPERATING_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: transitions![
            PingPongMsg::Pong(_) => stay,
        ],
    };

    pub(crate) const ACTIVE_FNS: StateFns<Self> = StateFns {
        on_entry: &[increment_round, send_initial_ping],
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
        on_entry: &[],
        on_exit: &[],
        transitions: &[],
    };

    pub(crate) const ERROR_FNS: StateFns<Self> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: &[],
    };
}
PING_ACTIONS

# ── Write Ping lib.rs (constants + module structure) ──────────────────────────
cat > crates/bloxes/ping/src/lib.rs <<'PING_LIB'
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

# ── Write Pong actions (simplified) ──────────────────────────────────────────
cat > crates/bloxes/pong/src/actions.rs <<'PONG_ACTIONS'
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

# ── Layer 5: Binary ──────────────────────────────────────────────────────────
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
ping-pong-actions  = { workspace = true }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-log = "0.2"
CRATE

cat > apps/tokio-demo/src/main.rs <<'MAIN'
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_tokio::prelude::*;
use ping_blox::prelude::*;
use ping_pong_messages::prelude::*;
use pong_blox::prelude::*;
use std::time::Duration;

#[derive(Debug, Default, Clone)]
struct DemoBehavior {
    round: u32,
    current_timer: Option<bloxide_timer::TimerId>,
}

impl ping_pong_actions::CountsRounds for DemoBehavior {
    type Round = u32;
    fn round(&self) -> u32 { self.round }
    fn set_round(&mut self, r: u32) { self.round = r; }
}

impl ping_pong_actions::HasCurrentTimer for DemoBehavior {
    fn current_timer(&self) -> Option<bloxide_timer::TimerId> { self.current_timer }
    fn set_current_timer(&mut self, t: Option<bloxide_timer::TimerId>) { self.current_timer = t; }
}

bloxide_tokio::actor_task_supervised!(ping_task, PingSpec<TokioRuntime, DemoBehavior>);
bloxide_tokio::actor_task_supervised!(pong_task, PongSpec<TokioRuntime>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

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

    let timer_ref = bloxide_tokio::spawn_timer!(8);

    let ((ping_ref,), ping_mbox) = bloxide_tokio::channels! {
        PingPongMsg(16),
    };
    let ping_id = ping_ref.id();

    let ((pong_ref,), pong_mbox) = bloxide_tokio::channels! {
        PingPongMsg(16),
    };
    let pong_id = pong_ref.id();

    let ping_ctx = PingCtx::new(
        ping_id,
        pong_ref.clone(),
        ping_ref.clone(),
        timer_ref,
        DemoBehavior::default(),
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
