// Copyright 2025 Bloxide, all rights reserved
//! Type-erased reply channel using Arc<dyn Fn>.
//!
//! This is TokioRuntime's implementation of erased reply channels.
//! Alloc is used here (in the RUNTIME), not in stdlib crates.

use std::any::{Any, TypeId};
use std::sync::Arc;

use bloxide_core::messaging::{ActorId, ActorRef};

use crate::{TokioRuntime, TokioTrySendError};

/// Type-erased sender using Arc<dyn Fn>.
// Allow complex type for erased sender pattern
#[allow(clippy::type_complexity)]
///
/// This is TokioRuntime's implementation of erased reply channels.
/// Alloc is used here (in the RUNTIME), not in stdlib crates.
pub struct ArcErasedSender {
    id: ActorId,
    send_fn:
        Arc<dyn Fn(ActorId, Box<dyn Any + Send>) -> Result<(), TokioTrySendError> + Send + Sync>,
    type_id: TypeId,
}

impl ArcErasedSender {
    /// Create a new erased sender from a typed ActorRef.
    pub fn new<M: Send + 'static>(sender: ActorRef<M, TokioRuntime>) -> Self {
        // Get the id before moving sender into the closure
        let id = sender.id();
        // Capture the sender in a closure that downcasts and sends
        let send_fn = Arc::new(move |from: ActorId, msg: Box<dyn Any + Send>| {
            // Downcast back to the concrete message type
            match msg.downcast::<M>() {
                Ok(typed_msg) => sender.try_send(from, *typed_msg),
                Err(_) => panic!(
                    "ArcErasedSender::send: message type mismatch - this is a bug in erased sender usage"
                ),
            }
        });

        Self {
            id,
            send_fn,
            type_id: TypeId::of::<M>(),
        }
    }

    /// Get the actor ID.
    pub fn id(&self) -> ActorId {
        self.id
    }

    /// Check if this sender accepts a specific message type.
    pub fn accepts<M: Send + 'static>(&self) -> bool {
        self.type_id == TypeId::of::<M>()
    }

    /// Send a type-erased message.
    ///
    /// # Panics
    ///
    /// Panics if the message type doesn't match the original type.
    pub fn send(&self, from: ActorId, msg: Box<dyn Any + Send>) -> Result<(), TokioTrySendError> {
        (self.send_fn)(from, msg)
    }
}

impl Clone for ArcErasedSender {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            send_fn: Arc::clone(&self.send_fn),
            type_id: self.type_id,
        }
    }
}

impl core::fmt::Debug for ArcErasedSender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ArcErasedSender")
            .field("id", &self.id)
            .field("type_id", &self.type_id)
            .finish()
    }
}
