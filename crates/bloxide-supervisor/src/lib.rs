// Copyright 2025 Bloxide, all rights reserved
#![no_std]

extern crate alloc;

// Hand-written trait impls for generated SupervisorCtx
pub mod ctx_impls;

// Custom Mailboxes for dynamic feature
#[cfg(feature = "dynamic")]
pub mod dynamic_mailboxes;

// Generated state machine code
pub mod generated;

// Tests
#[cfg(test)]
mod tests;

// Re-export types from bloxide-supervisor-context for backward compat
pub use bloxide_supervisor_context::{
    ChildAction, ChildGroup, ChildPolicy, GroupShutdown, HasChildGroup, HasChildGroupMut,
    HasChildNotify, HasPending, NoSpawnFactory, NoSpawnRequest, RegisterChild, RestartStrategy,
    SupervisorControl, SupervisorEvent, SupervisorEventLike,
};

// Re-export from generated
pub use generated::{SupervisorCtx, SupervisorSpec, SupervisorState};

// Re-export action functions from bloxide-supervisor-actions
pub use bloxide_supervisor_actions::{
    handle_done_or_failed, handle_health_check, record_stopped, start_children, stop_all_children,
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
    pub use bloxide_supervisor_context::{RegisterChild, SupervisorControl};
}
pub mod event {
    pub use bloxide_supervisor_context::{SupervisorEvent, SupervisorEventLike};
}
pub mod supervisor {
    pub use crate::generated::{SupervisorCtx, SupervisorSpec, SupervisorState};
}
