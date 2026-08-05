// Copyright 2025 Bloxide, all rights reserved.
//! Generate MachineSpec skeleton from actor, topology, context, and event config.
//!
//! When `context.feature` is set, emits paired `#[cfg]` variants with different
//! generics, event types, and mailboxes types. The StateFns are generated as
//! associated constants inside `impl` blocks using raw `StateRule { ... }` struct
//! literals emitted directly from TOML.

mod action;
mod emit;
mod state_fns;

pub(crate) use action::resolve_action;
pub use emit::generate;
// No crate-level callers outside `action` itself; re-exported so the original
// `crate::spec_skeleton::replace_placeholders` path keeps resolving.
#[allow(unused_imports)]
pub(crate) use action::replace_placeholders;
pub(crate) use state_fns::generate_state_fns_impl;
