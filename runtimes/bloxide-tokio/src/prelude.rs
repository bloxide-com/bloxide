/// Convenience re-exports for Tokio-based wiring sites.
///
/// Re-exports everything from `bloxide_core::prelude` plus Tokio-specific
/// runtime types. A single `use bloxide_tokio::prelude::*;` covers all
/// framework types needed in a wiring binary.
pub use crate::{
    run_actor, run_actor_to_completion, run_root, ChildGroupBuilder, SpawnCap, TokioRuntime,
    TokioSender, TokioStream,
};
pub use bloxide_core::prelude::*;
pub use bloxide_supervisor::registry::{ChildGroup, ChildPolicy, GroupShutdown};
pub use bloxide_supervisor::supervisor::{SupervisorCtx, SupervisorSpec, SupervisorState};
pub use bloxide_supervisor::{
    ChildLifecycleEvent, LifecycleCommand, SupervisedRunLoop, SupervisorEvent,
};
