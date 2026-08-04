// Copyright 2025 Bloxide, all rights reserved
//! Convenience re-exports for blox authors using the bloxide-codegen workflow.
//!
//! Covers the types needed to implement a `MachineSpec` with
//! `blox.toml`-generated state topology and events.
/// Import with `use bloxide_core::prelude::*;`.
pub use crate::{
    // Engine types (StateMachine for tests, DispatchOutcome for assertions)
    engine::{DispatchOutcome, StateMachine},
    // Mailbox types
    mailboxes::{Mailboxes, NoMailboxes},
    // Spec trait + handler table entry type
    spec::{MachineSpec, StateFns},
    // Topology types (StateTopology for path queries; LeafState for manual rules)
    topology::{LeafState, StateTopology},
    // Transition types (needed by action functions + StateRule literals;
    // ActionResults for any_failed()/all_ok() in guards)
    transition::{ActionFn, ActionResult, ActionResults, Decision, StateRule},
    // Identity and messaging
    ActorId,
    ActorRef,
    // Runtime trait (generic bound on context structs)
    BloxRuntime,
    Envelope,
    // Event infrastructure (needed by bloxide-codegen generated events and
    // manual event enum implementations)
    EventTag,
    LifecycleEvent,
    WILDCARD_TAG,
};
