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

/// Spawn request sent by the Pool to the Supervisor's spawn mailbox.
///
/// The Pool creates a typed reply channel and includes it in the request.
/// The factory sends a `SpawnedWorker` reply back on that channel.
#[derive(Debug, Clone)]
pub enum SpawnRequest<R: BloxRuntime> {
    /// Request to spawn a new worker actor.
    Worker {
        /// Task ID for the new worker.
        task_id: u32,
        /// Reply channel: the factory sends `SpawnedWorker` here.
        reply_to: ActorRef<SpawnedWorker<R>, R>,
        /// Pool ref the worker needs to send results back.
        pool_ref: ActorRef<PoolMsg, R>,
    },
}

/// Reply from the spawn factory containing the newly spawned worker's refs.
///
/// Sent by the factory back to the Pool via the `reply_to` channel in
/// `SpawnRequest`. The Pool uses these refs to send `DoWork` and
/// introduce peers.
///
/// The `ctrl_ref` now uses the generic `PeerCtrl<WorkerMsg, R>` from
/// `bloxide-peers` instead of a domain-specific `WorkerCtrl`.
#[derive(Debug, Clone)]
pub struct SpawnedWorker<R: BloxRuntime> {
    /// Actor ID of the spawned worker.
    pub child_id: ActorId,
    /// Domain message channel ref (for `WorkerMsg`).
    pub domain_ref: ActorRef<WorkerMsg, R>,
    /// Control channel ref (for `PeerCtrl<WorkerMsg, R>`).
    pub ctrl_ref: ActorRef<bloxide_peers::PeerCtrl<WorkerMsg, R>, R>,
}
