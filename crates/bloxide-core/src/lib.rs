// Copyright 2025 Bloxide, all rights reserved
#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[macro_use]
pub mod tracing;
pub mod capability;
pub mod engine;
pub mod event_tag;
pub mod lifecycle;
pub mod mailboxes;
pub mod messaging;
pub mod prelude;
pub mod runloop;
pub mod spec;
pub mod supervision;
pub mod topology;
pub mod transition;

// Tuple `Mailboxes` impls (arities 1..=16) are generated at build time by
// build.rs from the `[mailboxes]` section of blox.toml (via bloxide-codegen)
// and included from OUT_DIR. They are blanket trait impls, so there is
// nothing to name-import; the glob re-export mirrors the old
// `generated::mailboxes_impls` surface.
mod mailboxes_impls {
    include!(concat!(env!("OUT_DIR"), "/mailboxes_impls.rs"));
}
#[allow(unused_imports)]
pub use mailboxes_impls::*;

#[cfg(test)]
mod tests;

pub use capability::{
    BloxRuntime, DynamicChannelCap, GroupChannelCap, KillCapability, NoKill, StaticChannelCap,
};
pub use engine::{DispatchOutcome, MachineState, StateMachine};
pub use event_tag::{EventTag, LifecycleEvent, LIFECYCLE_TAG, WILDCARD_TAG};
pub use lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand};
pub use mailboxes::{Mailboxes, NoMailboxes};
pub use messaging::{ActorId, ActorRef, Envelope};
pub use runloop::{run, RunConfig};
pub use spec::{MachineSpec, StateFns};
pub use supervision::report_outcome;
pub use topology::{LeafState, StateTopology};
pub use transition::{ActionFn, ActionResult, ActionResults, Decision, StateRule};
// Note: TransitionRule is public because StateRule is a type alias over it. Use
// StateRule<S> in user code.

// Re-export proc macros as canonical public API
pub use bloxide_macros::next_actor_id;
