// Copyright 2025 Bloxide, all rights reserved
#![no_std]

extern crate alloc;

// Hand-written action functions (concrete, take &SupervisorEvent<R> directly)
pub mod actions;

// Generated state machine code
pub mod generated;

// Tests
#[cfg(test)]
mod tests;

// Re-export types from bloxide-supervisor-context for backward compat
pub use bloxide_supervisor_context::{
    spawn_supervised_child, ChildAction, ChildGroup, ChildPolicy, GroupShutdown, HasChildGroup,
    HasChildGroupMut, HasChildNotify, HasPending, RegisterChild, RegisterDynamicChild,
    RestartStrategy, SupervisorControl, SupervisorRegistrar,
};

// Re-export from generated (SupervisorEvent now codegen-generated, not hand-written)
pub use generated::{SupervisorCtx, SupervisorEvent, SupervisorSpec, SupervisorState};

// Re-export action functions from the local actions module
pub use actions::{
    handle_done_or_failed, handle_health_check, handle_register_dynamic_child, handle_reset,
    record_alive, record_started, record_stopped, register_child, start_children,
    stop_all_children,
};

// Backward-compat module aliases so existing `bloxide_supervisor::registry::*`
// and `bloxide_supervisor::supervisor::*` and `bloxide_supervisor::event::*`
// and `bloxide_supervisor::control::*` paths still resolve.
pub mod registry {
    pub use bloxide_supervisor_context::{
        ChildAction, ChildGroup, ChildPolicy, GroupShutdown, HasChildGroup, HasChildGroupMut,
        HasPending, RestartStrategy,
    };
}
pub mod control {
    pub use bloxide_supervisor_context::{RegisterChild, RegisterDynamicChild, SupervisorControl};
}
pub mod event {
    pub use crate::generated::SupervisorEvent;
}
pub mod supervisor {
    pub use crate::generated::{SupervisorCtx, SupervisorSpec, SupervisorState};
}
