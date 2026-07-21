// Copyright 2025 Bloxide, all rights reserved
#![no_std]

extern crate alloc;

// Supervisor-specific control-plane types (RegisterChild, RegisterDynamicChild,
// SupervisorControl, SupervisorRegistrar)
pub mod control;

// Spawn accessor trait (HasChildNotify)
pub mod spawn;

// Hand-written action functions (concrete, take &SupervisorEvent<R> directly)
pub mod actions;

// Generated state machine code
pub mod generated;

// Tests
#[cfg(test)]
mod tests;

// Re-export child-management types from bloxide-child-management
pub use bloxide_child_management::{
    AbortCommand, ChildAction, ChildGroup, ChildPolicy, GroupShutdown, HasChildGroup,
    HasChildGroupMut, HasPending, RestartStrategy,
};

// Re-export supervisor-specific types from local modules
pub use control::{RegisterChild, RegisterDynamicChild, SupervisorControl, SupervisorRegistrar};
pub use spawn::HasChildNotify;

// Re-export from generated (SupervisorEvent now codegen-generated, not hand-written)
pub use generated::{SupervisorCtx, SupervisorEvent, SupervisorSpec, SupervisorState};

// Re-export action functions from the local actions module
pub use actions::{
    handle_done_or_failed, handle_health_check, handle_register_dynamic_child, record_aborted,
    record_alive, record_killed, record_started, record_stopped, register_child, start_children,
    stop_all_children,
};
