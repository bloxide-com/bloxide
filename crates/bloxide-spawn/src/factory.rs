// Copyright 2025 Bloxide, all rights reserved
//! Factory trait for spawning child actors.
//!
//! This trait is the blox-facing interface for dynamic actor creation.
//! Implementations live in impl crates (e.g., `tokio-pool-demo-impl`),
//! not in spawn or supervisor crates.

extern crate alloc;

use alloc::boxed::Box;
use core::any::Any;

use bloxide_core::messaging::ActorRef;
use bloxide_core::lifecycle::ChildLifecycleEvent;

use crate::capability::{SpawnCap, SpawnCapability};
use crate::output::SpawnOutput;

/// Factory trait for spawning actors that handle message type M.
///
/// Impl crates (Layer 3) implement this. Binary registers it with the runtime.
///
/// The factory receives typed params (M::Params) and returns SpawnOutput
/// with the lifecycle channel for supervisor registration.
pub trait SpawnFactoryFor<M, R>: Send + Sync
where
    M: SpawnCapability,
    R: SpawnCap,
{
    fn spawn(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: M::Params,
        reply_to: Option<R::ErasedReplyTo>,
    ) -> Option<SpawnOutput<R>>;
}

/// Type-erased factory for heterogenous storage.
///
/// This trait allows any `SpawnFactoryFor<M, R>` to be stored
/// in the runtime's registry via the blanket impl below.
pub trait ErasedSpawnFactory<R: SpawnCap>: Send + Sync + 'static {
    fn spawn_erased(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: Box<dyn Any + Send>,
        reply_to: Option<R::ErasedReplyTo>,
    ) -> Option<SpawnOutput<R>>;
}

/// Wrapper that captures M for type-erased storage.
///
/// This is the key to making the TypeId-based registry work:
/// the wrapper captures the message type M at registration time,
/// allowing the blanket impl to downcast params to M::Params.
///
/// The Sync bound on R is required because ErasedSpawnFactory requires Sync,
/// and the factory registry is global (shared across threads for Tokio).
pub struct FactoryWrapper<M, R, F>
where
    M: SpawnCapability,
    R: SpawnCap + Sync,
    F: SpawnFactoryFor<M, R>,
{
    inner: F,
    _marker: core::marker::PhantomData<(M, R)>,
}

impl<M, R, F> FactoryWrapper<M, R, F>
where
    M: SpawnCapability,
    R: SpawnCap + Sync,
    F: SpawnFactoryFor<M, R>,
{
    pub fn new(factory: F) -> Self {
        Self {
            inner: factory,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<M, R, F> ErasedSpawnFactory<R> for FactoryWrapper<M, R, F>
where
    M: SpawnCapability,
    R: SpawnCap + Sync,
    F: SpawnFactoryFor<M, R> + 'static,
{
    fn spawn_erased(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: Box<dyn Any + Send>,
        reply_to: Option<R::ErasedReplyTo>,
    ) -> Option<SpawnOutput<R>> {
        let typed_params = params.downcast::<M::Params>().ok()?;
        self.inner.spawn(supervisor_notify, *typed_params, reply_to)
    }
}
