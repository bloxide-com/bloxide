// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for the worker→pool reference.
//!
//! Provides the `HasPoolRef` accessor trait.  The trait definition lives
//! here (with the data contract), not in the actions crate.
#![no_std]

use bloxide_core::{capability::BloxRuntime, messaging::ActorRef, ActorId};
use pool_messages::{PoolMsg, WorkDone};

/// Accessor for worker contexts that hold a reference back to the pool.
///
/// Implemented by `WorkerCtx`. Used by `notify_pool_done`.
pub trait HasPoolRef<R: BloxRuntime> {
    fn pool_ref(&self) -> &ActorRef<PoolMsg, R>;
}

/// Send `WorkDone` to the pool when the worker finishes its task.
pub fn notify_pool_done<R: BloxRuntime>(
    self_id: ActorId,
    pool_ref: &ActorRef<PoolMsg, R>,
    task_id: u32,
    result: u32,
) {
    let _ = pool_ref.try_send(
        self_id,
        PoolMsg::WorkDone(WorkDone {
            worker_id: self_id,
            task_id,
            result,
        }),
    );
}
