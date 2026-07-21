// Copyright 2025 Bloxide, all rights reserved
#![no_std]

extern crate alloc;

// Supervisor-specific control-plane types (RegisterChild, RegisterDynamicChild,
// SupervisorControl, SupervisorRegistrar)
pub mod control;

// Spawn convenience wrapper (spawn_supervised_child, HasChildNotify)
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
pub use spawn::{spawn_supervised_child, HasChildNotify};

// Re-export from generated (SupervisorEvent now codegen-generated, not hand-written)
pub use generated::{SupervisorCtx, SupervisorEvent, SupervisorSpec, SupervisorState};

// Re-export action functions from the local actions module
pub use actions::{
    handle_done_or_failed, handle_health_check, handle_register_dynamic_child, record_aborted,
    record_alive, record_started, record_stopped, register_child, start_children,
    stop_all_children,
};

// Backward-compat module aliases so existing `bloxide_supervisor::registry::*`
// and `bloxide_supervisor::control::*` and `bloxide_supervisor::event::*`
// and `bloxide_supervisor::supervisor::*` paths still resolve.
pub mod registry {
    pub use bloxide_child_management::{
        ChildAction, ChildGroup, ChildPolicy, GroupShutdown, HasChildGroup, HasChildGroupMut,
        HasPending, RestartStrategy,
    };
}

// Backward-compat module alias for `bloxide_supervisor::supervisor::*`
pub mod supervisor {
    pub use crate::generated::{SupervisorCtx, SupervisorSpec, SupervisorState};
}

// Backward-compat module alias for `bloxide_supervisor::event::*`
pub mod event {
    pub use crate::SupervisorEvent;
}
