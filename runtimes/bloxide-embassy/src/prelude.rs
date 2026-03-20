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
pub use bloxide_core::prelude::*;
pub use bloxide_core::{ChildLifecycleEvent, LifecycleCommand};
pub use bloxide_supervisor::registry::{ChildGroup, ChildPolicy, GroupShutdown};
pub use bloxide_supervisor::supervisor::{SupervisorCtx, SupervisorSpec, SupervisorState};
pub use bloxide_supervisor::{RegisterChild, SupervisorControl, SupervisorEvent};
pub use embassy_executor::Spawner;
