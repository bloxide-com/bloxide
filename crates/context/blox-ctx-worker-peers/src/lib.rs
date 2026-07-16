// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for worker peer references.
//!
//! Provides the `HasWorkerPeers` delegatable behavior trait.  The trait
//! definition lives here (with the data contract), not in the actions crate.
#![no_std]
extern crate alloc;

use alloc::vec::Vec;
use bloxide_core::{capability::BloxRuntime, messaging::ActorRef};
use bloxide_macros::delegatable;
use pool_messages::WorkerMsg;

/// Accessor trait for worker contexts that hold peer refs.
///
/// Unlike the generic `HasPeers<M, R>`, this trait is specific to `WorkerMsg`,
/// so it only has `R` as a generic parameter — the context's own runtime param.
/// This allows `#[delegates(HasWorkerPeers<R>)]` to work.
#[delegatable]
pub trait HasWorkerPeers<R: BloxRuntime> {
    fn peers(&self) -> &[ActorRef<WorkerMsg, R>];
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>>;
}
