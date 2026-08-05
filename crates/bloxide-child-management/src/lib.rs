// Copyright 2025 Bloxide, all rights reserved
//! Child group tracking, reset policies, and health checking.
//!
//! This is the reusable platform primitive for managing supervised children.
//! It is not supervisor-specific — any blox that tracks child actors can use
//! `ChildGroup` directly. The supervisor is one such consumer; a custom
//! managing blox could use it without depending on `bloxide-supervisor`.
//!
//! Following the Platform Feature Pattern (spec 18), this crate also owns the
//! child-management control plane (`control`: `ChildCtrl`, `RegisterChild`,
//! `RegisterDynamicChild`) and the consumer-side action functions (`actions`)
//! that a managing blox wires into its topology.
//!
//! # Reliability contract: confirm-before-record
//!
//! Every control-plane send is `try_send`, and the error kind is
//! load-bearing:
//!
//! - **Closed** is definitive: the child's task is gone. The entry moves to
//!   the terminal `Gone` phase immediately — dead-channel evidence needs no
//!   acknowledgment.
//! - **Full** is transient: the entry keeps its current phase and the command
//!   is stashed in `pending_cmd`. `flush_pending` retries it on every event
//!   pass through the group, so a lost command is recovered as long as the
//!   managing blox keeps receiving events.
//!
//! Phase transitions happen only on confirmed delivery or on observed
//! lifecycle reports — never on attempted sends. Bookkeeping therefore never
//! diverges from reality by more than one in-flight command.
//!
//! Note on runtimes: Embassy's static channels never close, so the Closed
//! branch is exercised on Tokio/TestRuntime; on Embassy a dead child is
//! detected via health-check misses instead.

#![no_std]
extern crate alloc;

pub mod actions;
pub mod builder;
pub mod control;
mod group;
mod policy;
mod registrar;

pub use group::ChildGroup;
pub use policy::{ChildPolicy, GroupShutdown};
pub use registrar::RegistrationError;

// Re-export the generic builder
pub use builder::ChildGroupBuilder;

// Re-export the control-plane message types
pub use control::{ChildCtrl, RegisterChild, RegisterDynamicChild};
