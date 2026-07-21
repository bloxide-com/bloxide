// Copyright 2025 Bloxide, all rights reserved
//! Accessor traits for the worker pool domain.
//!
//! Trait definitions now live in dedicated context crates or platform crates.
//! This module re-exports them so existing imports (`pool_actions::traits::HasWorkers`)
//! continue to work.  New code should import directly from the source crates.

pub use blox_ctx_current_task::HasCurrentTask;
pub use blox_ctx_pool_ref::HasPoolRef;
pub use blox_ctx_workers::HasWorkers;

// Re-export the generic `HasPeers` from `bloxide-peers`.
// The old domain-specific `HasWorkerPeers` trait has been replaced by the
// generic `HasPeers<WorkerMsg, R>` from `bloxide-peers`.
pub use bloxide_peers::HasPeers;
