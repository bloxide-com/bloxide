// Copyright 2025 Bloxide, all rights reserved
//! Generic action functions for the worker pool domain.
//!
//! All functions are trait-bounded against accessor traits from this crate.
//! No concrete types appear here.
extern crate alloc;

use bloxide_core::{accessor::HasSelfId, capability::BloxRuntime};
use bloxide_peers::PeerCtrl;
use pool_messages::{PeerResult, PoolMsg, WorkDone, WorkerMsg};

use crate::traits::{HasCurrentTask, HasPeers, HasPoolRef};

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
