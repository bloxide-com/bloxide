#![no_std]

extern crate alloc;

pub mod actions;
pub mod event;
pub mod lifecycle;
pub mod prelude;
pub mod registry;
pub mod service;
pub mod supervisor;

pub use actions::HasChildren;
pub use event::SupervisorEvent;
pub use lifecycle::{ChildLifecycleEvent, LifecycleCommand};
pub use registry::{ChildAction, ChildGroup, ChildPolicy, GroupShutdown};
pub use service::SupervisedRunLoop;
pub use supervisor::{SupervisorCtx, SupervisorSpec, SupervisorState};
