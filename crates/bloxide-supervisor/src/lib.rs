// Copyright 2025 Bloxide, all rights reserved
#![no_std]

extern crate alloc;

pub mod actions;
pub mod control;
pub mod event;
pub mod prelude;
pub mod registry;
pub mod supervisor;

pub use actions::HasChildren;
pub use control::{RegisterChild, SupervisorControl};
pub use event::SupervisorEvent;
pub use registry::{ChildGroup, ChildPolicy, GroupShutdown};
pub use supervisor::{SupervisorCtx, SupervisorSpec, SupervisorState};
