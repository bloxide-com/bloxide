// Copyright 2025 Bloxide, all rights reserved
//! Factory traits for dynamically spawning child actors.
//!
//! `SpawnFactoryFor` is the blox-facing interface for dynamic actor creation.
//! Implementations live in impl crates (e.g., `tokio-pool-demo-impl`),
//! not in spawn or supervisor crates.

#![allow(unused)]
extern crate alloc;

use alloc::boxed::Box;
use core::any::Any;

use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::messaging::ActorRef;

use crate::capability::SpawnCap;
use crate::output::SpawnOutput;

/// Marker trait for message types that represent a spawnable peer capability.
pub trait SpawnCapability: 'static + Send {
    type Params: Clone + core::fmt::Debug + Send;
}

/// Typed factory for spawning actors of message type `M` on runtime `R`.
pub trait SpawnFactoryFor<M, R>: Send + Sync
where
    M: SpawnCapability,
    R: SpawnCap,
{
    fn spawn(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: M::Params,
    ) -> Option<SpawnOutput<R>>;
}

/// Type-erased factory for heterogeneous storage.
pub trait ErasedSpawnFactory<R: SpawnCap>: Send + Sync + 'static {
    fn spawn_erased(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: Box<dyn Any + Send>,
    ) -> Option<SpawnOutput<R>>;
}

/// Wrapper that captures `M` for type-erased storage.
pub struct FactoryWrapper<M, R, F>
where
    M: SpawnCapability,
    R: SpawnCap,
    F: SpawnFactoryFor<M, R>,
{
    inner: F,
    _marker: core::marker::PhantomData<fn() -> (M, R)>,
}

impl<M, R, F> FactoryWrapper<M, R, F>
where
    M: SpawnCapability,
    R: SpawnCap,
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
    R: SpawnCap,
    F: SpawnFactoryFor<M, R> + 'static,
{
    fn spawn_erased(
        &self,
        supervisor_notify: ActorRef<ChildLifecycleEvent, R>,
        params: Box<dyn Any + Send>,
    ) -> Option<SpawnOutput<R>> {
        let typed_params = params.downcast::<M::Params>().ok()?;
        self.inner.spawn(supervisor_notify, *typed_params)
    }
}
