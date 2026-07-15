// Copyright 2025 Bloxide, all rights reserved
//! Pure domain message types for the worker pool example.
//!
//! No runtime dependencies — only plain data.
#![no_std]

pub mod generated;

pub mod prelude {
    pub use crate::*;
}

pub use generated::*;

use bloxide_core::{ActorId, ActorRef, BloxRuntime};

/// Control messages for worker peer introduction.
/// Sent on a dedicated control channel alongside domain WorkerMsg.
pub enum WorkerCtrl<R: BloxRuntime> {
    /// Add a peer that can receive WorkerMsg.
    AddPeer(AddWorkerPeer<R>),
    /// Remove a peer by actor ID.
    RemovePeer(RemoveWorkerPeer),
}

pub struct AddWorkerPeer<R: BloxRuntime> {
    pub peer_ref: ActorRef<WorkerMsg, R>,
}

pub struct RemoveWorkerPeer {
    pub peer_id: ActorId,
}
