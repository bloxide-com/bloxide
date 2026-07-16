// Copyright 2025 Bloxide, all rights reserved
//! Domain context crate for the current-task behavior.
//!
//! Provides the `HasCurrentTask` delegatable behavior trait.  The trait
//! definition lives here (with the data contract), not in the actions crate.
#![no_std]

use bloxide_macros::delegatable;

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
