// Copyright 2025 Bloxide, all rights reserved
pub use crate::{
    run_actor, run_actor_auto_start, run_root, run_supervised_actor, ChildGroupBuilder,
    EmbassyRuntime, EmbassySender, EmbassyStream,
};
/// Convenience re-exports for Embassy-based wiring sites.
///
/// Re-exports everything from `bloxide_core::prelude` plus Embassy-specific
/// runtime types and `Spawner`. A single `use bloxide_embassy::prelude::*;`
/// covers all framework types needed in a wiring binary.
///
/// Supervisor types are NOT re-exported here — the runtime does not depend
/// on `bloxide-supervisor`. Apps that use the supervisor import it directly:
/// `use bloxide_supervisor::*;`
pub use bloxide_child_management::{ChildGroup, ChildPolicy, GroupShutdown};
pub use bloxide_core::prelude::*;
pub use bloxide_core::{ChildLifecycleEvent, LifecycleCommand};
pub use embassy_executor::Spawner;
