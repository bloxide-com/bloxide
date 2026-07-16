// Copyright 2025 Bloxide, all rights reserved
//! Accessor traits for the worker pool domain.
//!
//! Trait definitions now live in dedicated context crates.  This module
//! re-exports them so existing imports (`pool_actions::traits::HasWorkers`)
//! continue to work.  New code should import directly from the context crates.

pub use blox_ctx_current_task::HasCurrentTask;
pub use blox_ctx_pool_ref::HasPoolRef;
pub use blox_ctx_worker_peers::HasWorkerPeers;
pub use blox_ctx_workers::HasWorkers;
