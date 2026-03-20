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
pub mod lifecycle;
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
pub use actor::{run_actor, run_actor_auto_start, run_actor_to_completion};
pub use capability::{BloxRuntime, DynamicChannelCap, KillCap, StaticChannelCap};
pub use engine::{DispatchOutcome, MachineState, StateMachine};
pub use event_tag::{EventTag, LifecycleEvent, LIFECYCLE_TAG, WILDCARD_TAG};
pub use lifecycle::{ChildLifecycleEvent, LifecycleCommand};
pub use mailboxes::{Mailboxes, NoMailboxes};
pub use messaging::{ActorId, ActorRef, Envelope};
pub use spec::{MachineSpec, StateFns};
pub use topology::{LeafState, StateTopology};
pub use transition::{ActionResult, ActionResults, Guard, StateRule};
// Note: TransitionRule is an implementation detail. Use StateRule<S> as the public type.

// Re-export proc macros as canonical public API
pub use bloxide_macros::{next_actor_id, root_transitions, transitions};
