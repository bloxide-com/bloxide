// Copyright 2025 Bloxide, all rights reserved
/// Convenience re-exports for Tokio-based wiring sites.
///
/// Re-exports everything from `bloxide_core::prelude` plus Tokio-specific
/// runtime types. A single `use bloxide_tokio::prelude::*;` covers all
/// framework types needed in a wiring binary.
///
/// Supervisor types are NOT re-exported here — the runtime does not depend
/// on `bloxide-supervisor`. Apps that use the supervisor import it directly:
/// `use bloxide_supervisor::*;`
pub use crate::{
    run_actor, run_actor_auto_start, run_actor_to_completion, run_root,
    run_supervised_actor_with_abort, GenericChildGroupBuilder, SpawnCap, TokioRuntime, TokioSender,
    TokioStream,
};
pub use bloxide_child_management::{ChildGroup, ChildGroupBuilder, ChildPolicy, GroupShutdown};
pub use bloxide_core::prelude::*;
pub use bloxide_core::{ChildLifecycleEvent, LifecycleCommand};
