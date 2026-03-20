// Copyright 2025 Bloxide, all rights reserved
//! Accessor traits for the worker pool domain.
extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
};
use bloxide_macros::delegatable;
use pool_messages::WorkerCtrl;
use pool_messages::{PoolMsg, WorkerMsg};

/// Function pointer type for spawning a single worker actor.
///
/// The factory allocates channels, constructs and spawns the worker task,
/// and returns the worker's domain and ctrl `ActorRef`s to the caller.
/// Sending `DoWork` and introducing peers is handled by the caller (the pool)
/// after the factory returns, so the pool controls message ordering.
pub type WorkerSpawnFn<R> =
    fn(ActorId, &ActorRef<PoolMsg, R>) -> (ActorRef<WorkerMsg, R>, ActorRef<WorkerCtrl<R>, R>);

/// Accessor for contexts that hold a worker spawn factory.
///
/// Implemented by `PoolCtx`. Allows generic pool logic to create workers
/// without knowing the concrete worker type.
pub trait HasWorkerFactory<R: BloxRuntime> {
    fn worker_factory(&self) -> WorkerSpawnFn<R>;
}

/// Accessor for contexts that spawn and track workers.
///
/// Implemented by the pool's context. Enables generic action functions
/// to introduce workers and query the current pool state.
pub trait HasWorkers<R: BloxRuntime> {
    fn worker_refs(&self) -> &[ActorRef<WorkerMsg, R>];
    fn worker_refs_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>>;
    fn worker_ctrls(&self) -> &[ActorRef<WorkerCtrl<R>, R>];
    fn worker_ctrls_mut(&mut self) -> &mut Vec<ActorRef<WorkerCtrl<R>, R>>;
    fn pending(&self) -> u32;
    fn set_pending(&mut self, n: u32);
}

/// Accessor for worker contexts that hold a reference back to the pool.
///
/// Implemented by `WorkerCtx`. Used by `notify_pool_done`.
pub trait HasPoolRef<R: BloxRuntime> {
    fn pool_ref(&self) -> &ActorRef<PoolMsg, R>;
}

/// Behavior trait for a worker context that is processing a task.
///
/// Implemented by `WorkerCtx`. Used by `notify_pool_done` and `broadcast_to_peers`.
#[delegatable]
pub trait HasCurrentTask {
    fn task_id(&self) -> u32;
    fn set_task_id(&mut self, id: u32);
    fn result(&self) -> u32;
    fn set_result(&mut self, r: u32);
}
/// Accessor trait for worker contexts that hold peer refs.
///
/// Unlike the generic `HasPeers<M, R>`, this trait is specific to `WorkerMsg`,
/// so it only has `R` as a generic parameter - the context's own runtime param.
/// This allows `#[delegates(HasWorkerPeers<R>)]` to work.
#[delegatable]
pub trait HasWorkerPeers<R: BloxRuntime> {
    fn peers(&self) -> &[ActorRef<WorkerMsg, R>];
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>>;
}
