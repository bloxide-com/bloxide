// Copyright 2025 Bloxide, all rights reserved
/// Sentinel value for [`crate::TransitionRule::event_tag`] that bypasses the
/// fast-reject check — a rule with this tag always proceeds to the `matches`
/// call regardless of the incoming event's tag.
///
/// Use when a single rule must handle events from multiple variants. The
/// `transitions!` proc macro uses this automatically for wildcard patterns.
pub const WILDCARD_TAG: u8 = u8::MAX;

/// A fast discriminant tag for event enums, used by the engine to skip
/// transition rule evaluation before calling the `matches` function pointer.
///
/// Every event type used with `MachineSpec` implements this trait. The `u8`
/// tag is assigned by declaration order (0, 1, 2, ...). [`WILDCARD_TAG`]
/// (255) is reserved as the wildcard sentinel in [`crate::TransitionRule::event_tag`]
/// — a rule with this tag always proceeds to the `matches` call regardless of
/// the incoming event's tag.
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
