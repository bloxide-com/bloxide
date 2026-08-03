// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for the worker→pool reference.
//!
//! Provides the `notify_pool_done` and `broadcast_result` action functions.
//! The `HasPoolRef` accessor trait has been removed — action functions take
//! concrete parameters and return `ActionResult` (uniform contract).
#![no_std]

use bloxide_core::{
    capability::BloxRuntime, messaging::ActorRef, transition::ActionResult, ActorId,
};
use pool_messages::{PeerResult, PoolMsg, WorkDone, WorkerMsg};

/// Spawn request sent by the Pool to the spawn factory.
///
/// The Pool creates a typed reply channel and includes it in the request.
/// The factory sends a `SpawnedWorker` reply back on that channel.
///
/// Carries `ActorRef`s, so it lives in this domain context crate rather than
/// in `pool-messages` (plain data only — AGENTS.md invariant #3). Generic
/// over the control message type `Ctrl`; callers instantiate it with
/// `PeerCtrl<WorkerMsg, R>`.
#[derive(Debug, Clone)]
pub enum SpawnRequest<Ctrl: Send + 'static, R: BloxRuntime> {
    /// Request to spawn a new worker actor.
    Worker {
        /// Task ID for the new worker.
        task_id: u32,
        /// Reply channel: the factory sends `SpawnedWorker` here.
        reply_to: ActorRef<SpawnedWorker<Ctrl, R>, R>,
        /// Pool ref the worker needs to send results back.
        pool_ref: ActorRef<PoolMsg, R>,
    },
}

/// Reply from the spawn factory containing the newly spawned worker's refs.
///
/// Sent by the factory back to the Pool via the `reply_to` channel in
/// `SpawnRequest`. The Pool uses these refs to send `DoWork` and
/// introduce peers.
#[derive(Debug, Clone)]
pub struct SpawnedWorker<Ctrl: Send + 'static, R: BloxRuntime> {
    /// Actor ID of the spawned worker.
    pub child_id: ActorId,
    /// Domain message channel ref (for `WorkerMsg`).
    pub domain_ref: ActorRef<WorkerMsg, R>,
    /// Control channel ref (for `Ctrl`, typically `PeerCtrl<WorkerMsg, R>`).
    pub ctrl_ref: ActorRef<Ctrl, R>,
}

/// Send `WorkDone` to the pool when the worker finishes its task.
pub fn notify_pool_done<R: BloxRuntime>(
    self_id: ActorId,
    pool_ref: &ActorRef<PoolMsg, R>,
    task_id: u32,
    result: u32,
) -> ActionResult {
    ActionResult::from(pool_ref.try_send(
        self_id,
        PoolMsg::WorkDone(WorkDone {
            worker_id: self_id,
            task_id,
            result,
        }),
    ))
}

/// Broadcast this worker's result to all registered peers.
///
/// Thin domain wrapper over `bloxide_peers::broadcast_to_peers`: it owns the
/// `WorkerMsg::PeerResult` construction so the platform crate stays
/// domain-agnostic (spec 20).
pub fn broadcast_result<R: BloxRuntime>(
    self_id: ActorId,
    peers: &[ActorRef<WorkerMsg, R>],
    result: u32,
) -> ActionResult {
    bloxide_peers::broadcast_to_peers(
        self_id,
        peers,
        WorkerMsg::PeerResult(PeerResult {
            from_id: self_id,
            result,
        }),
    )
}
