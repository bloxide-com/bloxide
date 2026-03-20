// Copyright 2025 Bloxide, all rights reserved

/// Sentinel value for the `event_tag` field of `StateRule` that bypasses the
/// fast-reject check — a rule with this tag always proceeds to the `matches`
/// call regardless of the incoming event's tag.
///
/// Use when a single rule must handle events from multiple variants. The
/// `transitions!` proc macro uses this automatically for wildcard patterns.
pub const WILDCARD_TAG: u8 = u8::MAX;

/// Reserved event tag for lifecycle commands.
/// Must be less than WILDCARD_TAG.
pub const LIFECYCLE_TAG: u8 = 254;

/// A fast discriminant tag for event enums, used by the engine to skip
/// transition rule evaluation before calling the `matches` function pointer.
///
/// Every event type used with `MachineSpec` implements this trait. The `u8`
/// tag is assigned by declaration order (0, 1, 2, ...). [`WILDCARD_TAG`]
/// (255) is reserved as the wildcard sentinel in the `event_tag` field of `StateRule`
/// — a rule with this tag always proceeds to the `matches` call regardless
/// of the incoming event's tag.
///
/// Implement this trait via:
/// - `#[derive(EventTag)]` on the event enum (from `bloxide-macros`)
/// - The `blox_event!` declarative macro (generates the impl automatically)
/// - The `#[blox_event]` proc-macro attribute (generates the impl automatically)
pub trait EventTag {
    /// Returns the `u8` discriminant tag for this event variant.
    /// Tags are assigned by variant declaration order starting at 0.
    fn event_tag(&self) -> u8;
}

/// Trait for events that may carry lifecycle commands.
///
/// Events implementing this trait can wrap lifecycle commands (Start/Reset/Stop/Ping)
/// so they flow through dispatch() and are handled at the VirtualRoot level.
///
/// For supervised actors using the unified lifecycle model, the event type
/// should have a variant that wraps `LifecycleCommand` and implement both
/// `as_lifecycle_command` and `from_lifecycle`.
///
/// For domain-only actors, `as_lifecycle_command` returns None and
/// `from_lifecycle` is not applicable.
pub trait LifecycleEvent: EventTag {
    /// Returns the lifecycle command if this event wraps one.
    /// Returns None for domain events.
    fn as_lifecycle_command(&self) -> Option<crate::lifecycle::LifecycleCommand> {
        let _ = self;
        None
    }
}
