// Copyright 2025 Bloxide, all rights reserved
//! Pool context — tracks spawned workers and pending completions.
extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
};
use bloxide_macros::BloxCtx;
use pool_actions::traits::{HasWorkerFactory, HasWorkers, WorkerSpawnFn};
use pool_messages::{PoolMsg, WorkerCtrl, WorkerMsg};

/// Context for the Pool actor.
///
/// The pool holds `ActorRef`s for all spawned workers (keeping channels alive)
/// and tracks how many results are still pending. `self_ref` is cloned into
/// each worker's context so workers can send `WorkDone` back to the pool.
///
/// `#[derive(BloxCtx)]` uses naming conventions for auto-detection:
/// - `self_id: ActorId` → `impl HasSelfId` (auto-detected)
///
/// For fields that match a naming pattern but shouldn't generate a trait impl,
/// use `#[ctor]` to mark them as constructor parameters only:
/// - `self_ref` would generate `HasSelfRef`, but we don't need that trait
/// - `worker_factory` would generate `HasWorkerFactory`, which we impl manually
#[derive(BloxCtx)]
pub struct PoolCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    /// Pool's own ActorRef — cloned into each worker at spawn time so the
    /// worker can notify the pool when done. Also keeps the pool channel open.
    #[ctor]
    pub self_ref: ActorRef<PoolMsg, R>,
    /// Factory function injected at construction time; called to create and
    /// spawn a worker without pool-blox knowing the concrete worker type.
    #[ctor]
    pub worker_factory: WorkerSpawnFn<R>,
    /// Domain ActorRefs for all spawned workers (keeps their channels alive).
    pub worker_refs: Vec<ActorRef<WorkerMsg, R>>,
    /// Ctrl ActorRefs for all spawned workers (used for peer introduction).
    pub worker_ctrls: Vec<ActorRef<WorkerCtrl<R>, R>>,
    /// Number of workers whose `WorkDone` we are still waiting for.
    pub pending: u32,
}

impl<R: BloxRuntime> HasWorkerFactory<R> for PoolCtx<R> {
    fn worker_factory(&self) -> WorkerSpawnFn<R> {
        self.worker_factory
    }
}

impl<R: BloxRuntime> HasWorkers<R> for PoolCtx<R> {
    fn worker_refs(&self) -> &[ActorRef<WorkerMsg, R>] {
        &self.worker_refs
    }
    fn worker_refs_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>> {
        &mut self.worker_refs
    }
    fn worker_ctrls(&self) -> &[ActorRef<WorkerCtrl<R>, R>] {
        &self.worker_ctrls
    }
    fn worker_ctrls_mut(&mut self) -> &mut Vec<ActorRef<WorkerCtrl<R>, R>> {
        &mut self.worker_ctrls
    }
    fn pending(&self) -> u32 {
        self.pending
    }
    fn set_pending(&mut self, n: u32) {
        self.pending = n;
    }
}
