// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{capability::BloxRuntime, ActorId, ActorRef};
use bloxide_macros::BloxCtx;
use ping_pong_actions::HasPeerRef;
use ping_pong_messages::PingPongMsg;

/// Context for the Pong actor.
///
/// Generic over `R` (the runtime), injected by the wiring layer — the blox
/// crate never imports `EmbassyRuntime` or any impl crate.
///
/// `#[derive(BloxCtx)]` generates:
/// - `impl HasSelfId for PongCtx<R>`
/// - `impl HasPeerRef<R> for PongCtx<R>` via `fn peer_ref(&self) -> &ActorRef<PingPongMsg, R>`
/// - `fn new(self_id, peer_ref) -> Self`
#[derive(BloxCtx)]
pub struct PongCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
    pub peer_ref: ActorRef<PingPongMsg, R>,
}
