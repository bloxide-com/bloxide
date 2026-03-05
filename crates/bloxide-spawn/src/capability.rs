// Copyright 2025 Bloxide, all rights reserved
use core::future::Future;

use bloxide_core::capability::DynamicChannelCap;

/// Tier 2 capability for runtimes that support spawning actor tasks at runtime.
///
/// Extends `DynamicChannelCap` (which provides `alloc_actor_id` and `channel`).
/// Blox crates that need dynamic spawning declare `R: SpawnCap`.
/// Embassy does NOT implement this trait — use static wiring for Embassy.
pub trait SpawnCap: DynamicChannelCap {
    /// Spawn a future as an independent task.
    fn spawn(future: impl Future<Output = ()> + Send + 'static);
}
