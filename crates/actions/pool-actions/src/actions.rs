// Copyright 2025 Bloxide, all rights reserved
//! Generic action functions for the worker pool domain.
//!
//! All functions are trait-bounded against accessor traits from this crate.
//! No concrete types appear here.
extern crate alloc;

use bloxide_core::{accessor::HasSelfId, capability::BloxRuntime};
use bloxide_peers::{introduce_peers, PeerCtrl};
use pool_messages::{PeerResult, PoolMsg, WorkDone, WorkerMsg};

use crate::traits::{HasCurrentTask, HasPoolRef, HasPeers, HasWorkers};

/// Send `WorkDone` to the pool when the worker finishes its task.
///
/// Designed for use in `on_entry` of the worker's Done state.
pub fn notify_pool_done<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasPoolRef<R> + HasCurrentTask,
{
    let _ = ctx.pool_ref().try_send(
        ctx.self_id(),
        PoolMsg::WorkDone(WorkDone {
            worker_id: ctx.self_id(),
            task_id: ctx.task_id(),
            result: ctx.result(),
        }),
    );
}

/// Broadcast this worker's result to all registered peers.
///
/// Designed for use in `on_entry` of the worker's Done state, called before
/// `notify_pool_done` so peers receive results before the pool is notified.
pub fn broadcast_to_peers<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasCurrentTask + HasPeers<WorkerMsg, R>,
{
    let from = ctx.self_id();
    let result = ctx.result();
    let n = ctx.peers().len();
    for i in 0..n {
        let peer_ref = ctx.peers()[i].clone();
        let _ = peer_ref.try_send(
            from,
            WorkerMsg::PeerResult(PeerResult {
                from_id: from,
                result,
            }),
        );
    }
}

/// Introduce the most recently added worker to all previously added workers.
///
/// Call this after adding a new worker's refs to `HasWorkers` to wire only
/// the new worker to existing peers — avoiding the duplicate introductions
/// that would result from calling `introduce_all_workers` repeatedly.
///
/// Uses the generic `bloxide-peers::introduce_peers` helper with the
/// domain-specific `WorkerMsg` type.
pub fn introduce_new_worker<R, C>(ctx: &C)
where
    R: BloxRuntime,
    C: HasSelfId + HasWorkers<R>,
{
    let n = ctx.worker_refs().len();
    if n < 2 {
        return;
    }
    let new_idx = n - 1;
    let from = ctx.self_id();
    let new_id = ctx.worker_refs()[new_idx].id();
    let new_ref = ctx.worker_refs()[new_idx].clone();
    let new_ctrl = ctx.worker_ctrls()[new_idx].clone();
    for i in 0..new_idx {
        let old_id = ctx.worker_refs()[i].id();
        let old_ref = ctx.worker_refs()[i].clone();
        let old_ctrl = ctx.worker_ctrls()[i].clone();
        introduce_peers(
            from,
            new_id, new_ref.clone(), new_ctrl.clone(),
            old_id, old_ref.clone(), old_ctrl.clone(),
        );
    }
}

/// Apply a `PeerCtrl<WorkerMsg, R>` command to the context's peer collection.
///
/// Handles both `AddPeer` and `RemovePeer` variants.
pub fn apply_worker_control<R, C>(ctx: &mut C, ctrl: &PeerCtrl<WorkerMsg, R>)
where
    R: BloxRuntime,
    C: HasPeers<WorkerMsg, R>,
{
    match ctrl {
        PeerCtrl::AddPeer(add) => ctx.peers_mut().push(add.peer_ref.clone()),
        PeerCtrl::RemovePeer(remove) => {
            ctx.peers_mut().retain(|r| r.id() != remove.peer_id);
        }
    }
}
