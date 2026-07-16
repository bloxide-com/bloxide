// Copyright 2025 Bloxide, all rights reserved
#![no_std]

//! Messaging service traits for bloxide contexts.
//!
//! This crate provides the two fundamental messaging accessor traits that
//! blox contexts use to expose their actor mailbox references:
//!
//! - [`HasSelfRef`] — reference to this actor's own mailbox (for self-delivered
//!   messages such as timer callbacks).
//! - [`HasPeerRef`] — reference to a peer actor's mailbox.
//!
//! Both traits are generic over the runtime `R: BloxRuntime` and the message
//! type `M`, so a single trait definition serves every actor that sends
//! messages — regardless of which message enum it uses.
//!
//! # Field conventions
//!
//! The `#[derive(BloxCtx)]` macro auto-detects fields by naming convention:
//!
//! | Field | Trait |
//! |-------|-------|
//! | `self_ref: ActorRef<M, R>` | `HasSelfRef<R, M>` |
//! | `peer_ref: ActorRef<M, R>` | `HasPeerRef<R, M>` |
//!
//! # Example
//!
//! ```ignore
//! use bloxide_core::{BloxRuntime, messaging::ActorRef};
//! use bloxide_messaging::{HasSelfRef, HasPeerRef};
//! use ping_pong_messages::PingPongMsg;
//!
//! #[derive(bloxide_macros::BloxCtx)]
//! pub struct PingCtx<R: BloxRuntime, B> {
//!     pub self_id: bloxide_core::ActorId,
//!     pub self_ref: ActorRef<PingPongMsg, R>,
//!     pub peer_ref: ActorRef<PingPongMsg, R>,
//!     #[delegates(/* behavior traits */)]
//!     pub behavior: B,
//! }
//! ```

use bloxide_core::{capability::BloxRuntime, messaging::ActorRef};

/// Reference to this actor's own mailbox (for self-delivered messages).
///
/// Auto-generated from a `self_ref: ActorRef<M, R>` field by `#[derive(BloxCtx)]`.
/// Used by action functions that need to send messages to themselves (e.g.,
/// timer-delivered resume messages).
pub trait HasSelfRef<R: BloxRuntime, M: Send + 'static> {
    /// Returns a reference to this actor's own mailbox handle.
    fn self_ref(&self) -> &ActorRef<M, R>;
}

/// Reference to a peer actor's mailbox.
///
/// Auto-generated from a `peer_ref: ActorRef<M, R>` field by `#[derive(BloxCtx)]`.
/// Used by action functions that send messages to a peer actor.
pub trait HasPeerRef<R: BloxRuntime, M: Send + 'static> {
    /// Returns a reference to the peer actor's mailbox handle.
    fn peer_ref(&self) -> &ActorRef<M, R>;
}
