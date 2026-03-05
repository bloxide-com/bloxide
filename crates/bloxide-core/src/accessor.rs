/// Universal accessor traits for blox context types.
///
/// These traits are the building blocks for trait-bounded action functions.
/// Action crates (e.g., `bloxide-log`, `ping-pong-actions`) define
/// their behavior traits in terms of these universal accessors plus their own
/// domain-specific traits.
///
/// Implementing these traits on a context struct is typically done via the
/// `#[derive(BloxCtx)]` macro using `#[self_id]` and `#[provides(...)]` annotations.
use crate::messaging::ActorId;

/// Provides access to the actor's own `ActorId`.
///
/// Required by any action that needs to know the sender ID (e.g., for
/// `try_send(self_id, msg)` calls on outgoing channel refs).
pub trait HasSelfId {
    fn self_id(&self) -> ActorId;
}
