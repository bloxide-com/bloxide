// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{capability::BloxRuntime, ActorId, ActorRef};
use bloxide_macros::BloxCtx;
use bloxide_timer::{HasTimerRef, TimerCommand, TimerId};
use ping_pong_actions::{
    CountsRounds, HasCurrentTimer, HasPeerRef, HasSelfRef, __delegate_CountsRounds,
    __delegate_HasCurrentTimer,
};
use ping_pong_messages::PingPongMsg;

/// Context for the Ping actor.
///
/// Generic over `R` (the runtime), injected by the wiring layer — the blox
/// crate never imports `EmbassyRuntime` or any runtime crate or impl crate.
///
/// Generic over `B` (the behavior), a single composite type injected by the
/// wiring binary from an impl crate at construction time. `B` must implement
/// both behavior traits: `HasCurrentTimer` and `CountsRounds`.
///
/// Accessor traits (`HasPeerRef`, `HasSelfRef`, `HasTimerRef`) are auto-generated
/// by `#[derive(BloxCtx)]` based on naming conventions (`*_ref` fields).
/// Behavior traits are delegated to `self.behavior`.
#[derive(BloxCtx)]
pub struct PingCtx<R: BloxRuntime, B: HasCurrentTimer + CountsRounds> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,

    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
