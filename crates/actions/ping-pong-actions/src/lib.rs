// Copyright 2025 Bloxide, all rights reserved
//! Domain action traits and generic functions for the ping-pong example.
//!
//! This crate is a portable action crate: it defines traits and trait-bounded
//! generic functions without concrete runtime types.
//!
//! # Layer responsibilities
//!
//! - **Accessor traits** (`HasPeerRef`, `HasSelfRef`): expose the `ActorRef<PingPongMsg, R>`
//!   handles that actions need to send messages. Both Ping and Pong share `PingPongMsg`.
//!
//! - **Behavior traits** (`CountsRounds`, `HasCurrentTimer`): define what data the context
//!   exposes to actions. Implemented on context structs in the blox crate (simple field
//!   delegation).
//!
//! - **Generic action functions**: trait-bounded fns composable into `on_entry`/`on_exit`
//!   slices and transition action lists. All work against trait bounds — no concrete types.

#![no_std]

use bloxide_core::{
    accessor::HasSelfId, capability::BloxRuntime, messaging::ActorRef, transition::ActionResult,
};
use bloxide_macros::delegatable;
use bloxide_timer::{cancel_timer, set_timer, HasTimerRef, TimerId};
use ping_pong_messages::{Ping, PingPongMsg, Pong, Resume};

// ── Accessor traits ───────────────────────────────────────────────────────────

/// Provides access to the peer actor's mailbox (the other actor in the pair).
///
/// Both Ping and Pong use `PingPongMsg`, so the peer ref type is the same
/// for both actors.
pub trait HasPeerRef<R: BloxRuntime> {
    fn peer_ref(&self) -> &ActorRef<PingPongMsg, R>;
}

/// Provides access to this actor's own mailbox (for timer-delivered self messages).
pub trait HasSelfRef<R: BloxRuntime> {
    fn self_ref(&self) -> &ActorRef<PingPongMsg, R>;
}

// ── Behavior traits ───────────────────────────────────────────────────────────

/// Tracks the current round number in the ping-pong exchange.
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

/// Provides read/write access to the current pending timer ID.
#[delegatable]
pub trait HasCurrentTimer {
    fn current_timer(&self) -> Option<TimerId>;
    fn set_current_timer(&mut self, timer: Option<TimerId>);
}

// ── Entry/exit action functions ────────────────────────────────────────────────
//
// These are infallible (no return value) and composable into `on_entry`/`on_exit`
// &'static [fn(&mut Ctx)] slices.

/// Increment the round counter. Use in `Active::on_entry`.
pub fn increment_round<C: CountsRounds>(ctx: &mut C) {
    let one = C::Round::from(1);
    ctx.set_round(ctx.round() + one);
}

/// Schedule a resume timer delivering `PingPongMsg::Resume` to self after
/// `duration_ms` milliseconds. Stores the `TimerId` in the context via
/// `HasCurrentTimer`; no return value since the ID is always stored.
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

/// Cancel the current pending timer (if any) and clear the stored ID.
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

// ── Transition action functions ────────────────────────────────────────────────

/// Send a `PingPongMsg::Ping` to the peer with the current round.
/// Used as a transition action (fallible) in both the Pong-response and
/// Resume-from-Paused paths.
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

/// Entry action: send a Ping only on the initial entry to Active (round == 1).
/// On subsequent re-entries (self-transitions and Resume) the ping is sent by
/// the transition action instead.
pub fn send_initial_ping<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasPeerRef<R> + CountsRounds,
    C::Round: Into<u32>,
{
    if ctx.round() == C::Round::from(1) {
        // This is called from on_entry before the peer is guaranteed to be running.
        // A channel-full error at startup is non-fatal (the round will not begin) but
        // is logged as a warning so it is visible in diagnostics.
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

/// Send a `PingPongMsg::Pong` to the peer echoing the received round number.
/// Called from Pong's Ready state when it receives a Ping message.
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
