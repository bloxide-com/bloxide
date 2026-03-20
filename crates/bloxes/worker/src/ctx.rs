// Copyright 2025 Bloxide, all rights reserved
//! Worker context — holds runtime refs and task state.
extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
};
use bloxide_macros::BloxCtx;
use pool_actions::{
    __delegate_HasCurrentTask, __delegate_HasWorkerPeers,
    traits::{HasCurrentTask, HasPoolRef, HasWorkerPeers},
};
use pool_messages::{PoolMsg, WorkerMsg};

/// Context for the Worker actor.
///
/// Generic over `R` (the runtime) and `B` (behavior type) — the blox crate
/// never imports any concrete runtime or behavior. The wiring layer injects
/// both at construction time.
///
/// `#[derive(BloxCtx)]` generates:
/// - `impl HasSelfId for WorkerCtx<R, B>` (auto-detected from `self_id: ActorId`)
/// - `impl HasPoolRef<R> for WorkerCtx<R, B>` (auto-detected from `pool_ref` field)
/// - `impl HasCurrentTask` via `#[delegates]`
/// - `fn new(self_id, pool_ref, behavior) -> Self`
///
/// Note: With the domain-specific `HasWorkerPeers<R>` trait, `#[delegates]` now works
/// without manual impls.
#[derive(BloxCtx)]
pub struct WorkerCtx<R: BloxRuntime, B: HasWorkerPeers<R> + HasCurrentTask> {
    pub self_id: ActorId,
    pub pool_ref: ActorRef<PoolMsg, R>,

    #[delegates(HasWorkerPeers<R>, HasCurrentTask)]
    pub behavior: B,
}
