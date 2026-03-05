// Copyright 2025 Bloxide, all rights reserved
/// Convenience re-exports for blox authors using the macro-based workflow.
///
/// Covers the types needed to implement a `MachineSpec` with `#[derive(BloxCtx)]`,
/// `#[derive(StateTopology)]`, `#[blox_event]`, and `transitions!`.
/// Import with `use bloxide_core::prelude::*;`.
pub use crate::{
    // Engine types (StateMachine for tests, DispatchOutcome for assertions)
    engine::{DispatchOutcome, StateMachine},
    // Mailbox types
    mailboxes::{Mailboxes, NoMailboxes},
    root_transitions,
    // Spec trait + handler table entry type
    spec::{MachineSpec, StateFns},
    // Topology types (StateTopology for path queries; LeafState for manual rules)
    topology::{LeafState, StateTopology},
    // Transition types (needed by transitions! macro output + action functions)
    transition::{ActionFn, ActionResult, Guard, StateRule},
    // Declarative transition macros
    transitions,
    // Identity and messaging
    ActorId,
    ActorRef,
    // Runtime trait (generic bound on context structs)
    BloxRuntime,
    Envelope,
    // Event infrastructure (needed by #[blox_event] generated code)
    EventTag,
    HasSelfId,
    WILDCARD_TAG,
};
