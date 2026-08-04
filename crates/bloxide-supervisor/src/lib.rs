// Copyright 2025 Bloxide, all rights reserved
//! The standard supervisor blox — the reference consumer of the
//! child-management platform feature (spec 18: Platform Feature Pattern).
//!
//! This crate owns only the supervisor topology: `blox.toml`, the generated
//! state machine code, and tests. The child-management control plane
//! (`ChildCtrl`, `RegisterChild`, `RegisterDynamicChild`), the consumer-side
//! action functions, and the `ChildGroup` tracking primitive all live in
//! `bloxide-child-management`. The spawn registration bridge
//! (`ChildCtrlRegistrar`) lives in `bloxide-spawn`.
#![no_std]

extern crate alloc;

// Concrete supervisor spec for in-crate testing (wires platform action
// functions). System-level apps get their own concrete spec from the codegen.
pub mod concrete_spec;

// Generated state machine code
pub mod generated;

// Tests
#[cfg(test)]
mod tests;

// Re-export the blox's own generated types
pub use generated::{SupervisorCtx, SupervisorEvent, SupervisorSpec, SupervisorState};
