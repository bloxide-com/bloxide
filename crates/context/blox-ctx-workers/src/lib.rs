// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for worker-pool capabilities.
//!
//! Provides the accessor traits and impl macro for contexts that spawn and
//! track workers.  Trait definitions live here (with the data contract), not
//! in the actions crate.
#![no_std]
extern crate alloc;

// Re-export the types the impl_has_workers! macro references via $crate::
pub use alloc::vec::Vec;
pub use bloxide_core::{
    capability::BloxRuntime,
    messaging::{ActorId, ActorRef},
};
pub use pool_messages::{PoolMsg, WorkerCtrl, WorkerMsg};

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

/// Generate a `HasWorkers` impl for a context type.
///
/// The context struct must have fields named `worker_refs`, `worker_ctrls`,
/// and `pending` with the types expected by `HasWorkers`.
///
/// ```ignore
/// impl_has_workers!(PoolCtx<R>);
/// ```
#[macro_export]
macro_rules! impl_has_workers {
    ($ctx:ident<$R:ident>) => {
        impl<$R: $crate::BloxRuntime> $crate::HasWorkers<$R> for $ctx<$R> {
            fn worker_refs(&self) -> &[$crate::ActorRef<$crate::WorkerMsg, $R>] {
                &self.worker_refs
            }
            fn worker_refs_mut(&mut self) -> &mut $crate::Vec<$crate::ActorRef<$crate::WorkerMsg, $R>> {
                &mut self.worker_refs
            }
            fn worker_ctrls(&self) -> &[$crate::ActorRef<$crate::WorkerCtrl<$R>, $R>] {
                &self.worker_ctrls
            }
            fn worker_ctrls_mut(
                &mut self,
            ) -> &mut $crate::Vec<$crate::ActorRef<$crate::WorkerCtrl<$R>, $R>> {
                &mut self.worker_ctrls
            }
            fn pending(&self) -> u32 {
                self.pending
            }
            fn set_pending(&mut self, n: u32) {
                self.pending = n;
            }
        }
    };
}
