// Copyright 2025 Bloxide, all rights reserved
//! Peer introduction control messages and helpers.
//!
//! Provides the generic `PeerCtrl<M, R>` control message type and the
//! `introduce_peers` / `apply_peer_control` / `broadcast_to_peers` helper
//! functions. All helpers are domain-agnostic (spec 18): the message
//! broadcast to peers is supplied by the caller. Domain code uses these
//! directly instead of defining per-domain copies like `WorkerCtrl`,
//! `AddWorkerPeer`, etc.

#![no_std]
extern crate alloc;
use alloc::vec::Vec;
use core::fmt;

use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
    transition::ActionResult,
};

/// Control message for managing a peer collection.
pub enum PeerCtrl<M: Send + 'static, R: BloxRuntime> {
    /// Add a peer to the collection.
    AddPeer(AddPeer<M, R>),
    /// Remove a peer by actor ID.
    RemovePeer(RemovePeer),
}

/// Request to add a peer.
pub struct AddPeer<M: Send + 'static, R: BloxRuntime> {
    pub peer_id: ActorId,
    pub peer_ref: ActorRef<M, R>,
}

/// Request to remove a peer.
pub struct RemovePeer {
    pub peer_id: ActorId,
}

impl<M: Send + 'static, R: BloxRuntime> Clone for AddPeer<M, R> {
    fn clone(&self) -> Self {
        Self {
            peer_id: self.peer_id,
            peer_ref: self.peer_ref.clone(),
        }
    }
}

impl Clone for RemovePeer {
    fn clone(&self) -> Self {
        Self {
            peer_id: self.peer_id,
        }
    }
}

impl<M: Send + 'static, R: BloxRuntime> Clone for PeerCtrl<M, R> {
    fn clone(&self) -> Self {
        match self {
            Self::AddPeer(add) => Self::AddPeer(add.clone()),
            Self::RemovePeer(remove) => Self::RemovePeer(remove.clone()),
        }
    }
}

impl<M: Send + 'static, R: BloxRuntime> fmt::Debug for AddPeer<M, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddPeer")
            .field("peer_id", &self.peer_id)
            .field("peer_ref_id", &self.peer_ref.id())
            .finish()
    }
}

impl fmt::Debug for RemovePeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemovePeer")
            .field("peer_id", &self.peer_id)
            .finish()
    }
}

impl<M: Send + 'static, R: BloxRuntime> fmt::Debug for PeerCtrl<M, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddPeer(add) => f.debug_tuple("AddPeer").field(add).finish(),
            Self::RemovePeer(remove) => f.debug_tuple("RemovePeer").field(remove).finish(),
        }
    }
}

/// Introduce two actors to each other by sending `AddPeer` on both control channels.
///
/// Best-effort: both sends are always attempted and the combined result is
/// returned, but a failure is not rolled back. If the second send fails
/// after the first succeeded, the peers are left asymmetrically introduced
/// (`a` knows `b`, but `b` does not know `a`) — callers that need symmetry
/// must retry or unwind the first introduction themselves.
pub fn introduce_peers<M, R>(
    from: ActorId,
    a_id: ActorId,
    a_ref: ActorRef<M, R>,
    a_ctrl: ActorRef<PeerCtrl<M, R>, R>,
    b_id: ActorId,
    b_ref: ActorRef<M, R>,
    b_ctrl: ActorRef<PeerCtrl<M, R>, R>,
) -> ActionResult
where
    M: Send + 'static,
    R: BloxRuntime,
{
    let r1 = a_ctrl.try_send(
        from,
        PeerCtrl::AddPeer(AddPeer {
            peer_id: b_id,
            peer_ref: b_ref.clone(),
        }),
    );
    let r2 = b_ctrl.try_send(
        from,
        PeerCtrl::AddPeer(AddPeer {
            peer_id: a_id,
            peer_ref: a_ref.clone(),
        }),
    );
    ActionResult::from(r1.and(r2))
}

/// Apply a `PeerCtrl` command to a peer collection.
///
/// Handles both `AddPeer` and `RemovePeer` variants. `AddPeer` is idempotent:
/// a peer whose id is already present is not added again.
pub fn apply_peer_control<M, R>(
    peers: &mut Vec<ActorRef<M, R>>,
    ctrl: &PeerCtrl<M, R>,
) -> ActionResult
where
    M: Send + 'static,
    R: BloxRuntime,
{
    match ctrl {
        PeerCtrl::AddPeer(add) => {
            if !peers.iter().any(|r| r.id() == add.peer_id) {
                peers.push(add.peer_ref.clone());
            }
        }
        PeerCtrl::RemovePeer(remove) => {
            peers.retain(|r| r.id() != remove.peer_id);
        }
    }
    ActionResult::Ok
}

/// Broadcast a message to all registered peers. `Err` if any send fails
/// (sends continue best-effort past the first failure).
pub fn broadcast_to_peers<M, R>(self_id: ActorId, peers: &[ActorRef<M, R>], msg: M) -> ActionResult
where
    M: Clone + Send + 'static,
    R: BloxRuntime,
{
    let mut result = ActionResult::Ok;
    for peer_ref in peers {
        let r = peer_ref.try_send(self_id, msg.clone());
        if r.is_err() {
            result = ActionResult::from(r);
        }
    }
    result
}

#[cfg(test)]
mod tests;
