// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for the worker→pool reference.
//!
//! Provides the `HasPoolRef` accessor trait.  The trait definition lives
//! here (with the data contract), not in the actions crate.
#![no_std]

use bloxide_core::{capability::BloxRuntime, messaging::ActorRef};
use pool_messages::PoolMsg;

/// Accessor for worker contexts that hold a reference back to the pool.
///
/// Implemented by `WorkerCtx`. Used by `notify_pool_done`.
pub trait HasPoolRef<R: BloxRuntime> {
    fn pool_ref(&self) -> &ActorRef<PoolMsg, R>;
}
