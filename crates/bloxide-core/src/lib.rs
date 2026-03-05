// Copyright 2025 Bloxide, all rights reserved
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

#[macro_use]
pub mod tracing;
pub mod accessor;
pub mod actor;
pub mod capability;
pub mod engine;
pub mod event_tag;
pub mod mailboxes;
pub mod messaging;
pub mod prelude;
pub mod spec;
#[cfg(feature = "std")]
pub mod test_utils;
pub mod topology;
pub mod transition;

#[cfg(test)]
mod tests;

pub use accessor::HasSelfId;
pub use actor::{run_actor, run_actor_to_completion};
pub use capability::{BloxRuntime, DynamicChannelCap, StaticChannelCap};
pub use engine::{DispatchOutcome, MachinePhase, StateMachine};
pub use event_tag::{EventTag, WILDCARD_TAG};
pub use mailboxes::{Mailboxes, NoMailboxes};
pub use messaging::{ActorId, ActorRef, Envelope};
pub use spec::{MachineSpec, StateFns};
pub use topology::{LeafState, StateTopology};
pub use transition::{ActionResult, ActionResults, Guard, StateRule, TransitionRule};

// Re-export proc macros as canonical public API
pub use bloxide_macros::{next_actor_id, root_transitions, transitions};
