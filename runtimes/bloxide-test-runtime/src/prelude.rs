// Copyright 2025 Bloxide, all rights reserved
/// Convenience re-exports for TestRuntime-based wiring sites (unit tests).
///
/// Re-exports everything from `bloxide_core::prelude` plus the test-runtime
/// types tests reach for most: the runtime handle, channel types and errors,
/// the run-loop entry points, and the spawn/kill log helpers used to assert
/// on the observable spawn/kill behavior. A single
/// `use bloxide_test_runtime::prelude::*;` covers the framework types needed
/// in a unit test.
///
/// Supervisor types are NOT re-exported here — the runtime does not depend
/// on `bloxide-supervisor`. Tests that use the supervisor import it directly:
/// `use bloxide_supervisor::*;`
pub use crate::{
    drain_killed, drain_spawned, kill_count, spawned_count, TestReceiver, TestRuntime,
    TestSendError, TestSender, TestTrySendError,
};
pub use bloxide_core::capability::DynamicChannelCap;
pub use bloxide_core::prelude::*;
pub use bloxide_core::{run, AbortCommand, ChildLifecycleEvent, LifecycleCommand, RunConfig};
pub use bloxide_spawn::SpawnCap;
