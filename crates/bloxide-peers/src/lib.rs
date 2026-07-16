// Copyright 2025 Bloxide, all rights reserved
//! Peer introduction control messages and helper.

#![no_std]
extern crate alloc;
use alloc::vec::Vec;
use core::fmt;

use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
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

/// Accessor trait for contexts that track a collection of peer refs.
pub trait HasPeers<M: Send + 'static, R: BloxRuntime> {
    /// Returns the current peer refs.
    fn peers(&self) -> &[ActorRef<M, R>];
    /// Returns the mutable peer collection.
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<M, R>>;
}

/// Introduce two actors to each other by sending `AddPeer` on both control channels.
pub fn introduce_peers<M, R>(
    from: ActorId,
    a_id: ActorId,
    a_ref: ActorRef<M, R>,
    a_ctrl: ActorRef<PeerCtrl<M, R>, R>,
    b_id: ActorId,
    b_ref: ActorRef<M, R>,
    b_ctrl: ActorRef<PeerCtrl<M, R>, R>,
) where
    M: Send + 'static,
    R: BloxRuntime,
{
    let _ = a_ctrl.try_send(
        from,
        PeerCtrl::AddPeer(AddPeer {
            peer_id: b_id,
            peer_ref: b_ref.clone(),
        }),
    );
    let _ = b_ctrl.try_send(
        from,
        PeerCtrl::AddPeer(AddPeer {
            peer_id: a_id,
            peer_ref: a_ref.clone(),
        }),
    );
}
