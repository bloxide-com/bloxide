// Copyright 2025 Bloxide, all rights reserved
#![no_std]
extern crate alloc;

pub mod control;
pub mod event;
pub mod registry;
pub mod spawn;

pub use control::{RegisterChild, RegisterDynamicChild, SupervisorControl, SupervisorRegistrar};
pub use event::{SupervisorEvent, SupervisorEventLike};
pub use registry::{
    ChildAction, ChildGroup, ChildPolicy, GroupShutdown, HasChildGroup, HasChildGroupMut,
    HasPending, RestartStrategy,
};
pub use spawn::{spawn_supervised_child, HasChildNotify};
