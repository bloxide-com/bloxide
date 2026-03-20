// Copyright 2025 Bloxide, all rights reserved.
//! Pure domain message types for the worker pool example.
//!
//! No runtime dependencies — only plain data.
#![no_std]

pub mod prelude {
    pub use crate::*;
}

use bloxide_macros::blox_messages;
use bloxide_core::{ActorId, ActorRef, BloxRuntime};

// Messages sent to the pool actor.
blox_messages! {
    pub enum PoolMsg {
        SpawnWorker { task_id: u32 },
        WorkDone { worker_id: usize, task_id: u32, result: u32 },
    }
}

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

// Messages sent to a worker actor.
blox_messages! {
    pub enum WorkerMsg {
        DoWork { task_id: u32 },
        PeerResult { from_id: usize, result: u32 },
    }
}
