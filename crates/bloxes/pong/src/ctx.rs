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
/// - `impl HasSelfId for PongCtx<R>` (auto-detected from `self_id: ActorId`)
/// - `impl HasPeerRef<R> for PongCtx<R>` (auto-detected from `peer_ref: ActorRef<M, R>`)
/// - `fn new(self_id, peer_ref) -> Self`
#[derive(BloxCtx)]
pub struct PongCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
}
