// Copyright 2025 Bloxide, all rights reserved
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

#[macro_use]
pub mod tracing;
pub mod capability;
pub mod engine;
pub mod event_tag;
pub mod generated;
pub mod lifecycle;
pub mod mailboxes;
pub mod messaging;
pub mod prelude;
pub mod runloop;
pub mod spec;
pub mod supervision;
pub mod topology;
pub mod transition;

#[cfg(test)]
mod tests;

pub use capability::{BloxRuntime, DynamicChannelCap, KillCapability, NoKill, StaticChannelCap};
pub use engine::{DispatchOutcome, MachineState, StateMachine};
pub use event_tag::{EventTag, LifecycleEvent, LIFECYCLE_TAG, WILDCARD_TAG};
pub use lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand};
pub use mailboxes::{Mailboxes, NoMailboxes};
pub use messaging::{ActorId, ActorRef, Envelope};
pub use runloop::{run, RunConfig};
pub use spec::{MachineSpec, StateFns};
pub use supervision::report_outcome;
pub use topology::{LeafState, StateTopology};
pub use transition::{ActionResult, ActionResults, Decision, StateRule};
// Note: TransitionRule is public because StateRule is a type alias over it. Use
// StateRule<S> in user code.

// Re-export proc macros as canonical public API
pub use bloxide_macros::next_actor_id;
