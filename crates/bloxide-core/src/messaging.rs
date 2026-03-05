// Copyright 2025 Bloxide, all rights reserved
use crate::capability::BloxRuntime;

/// Unique actor identifier. Assigned statically by the wiring crate.
pub type ActorId = usize;

/// A message delivered to an actor's mailbox.
///
/// `Envelope(from, payload)` — `from` is the sender's `ActorId`, `payload` is the message.
///
/// # Pattern matching
///
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// // Ignore the sender (common case):
/// PingEvent::Msg(Envelope(_, PingMsg::Pong(Pong { round }))) => { ... }
///
/// // Match on sender when needed:
/// PingEvent::Msg(Envelope(from, PingMsg::Pong(Pong { round }))) => { ... }
/// ```
#[derive(Debug)]
pub struct Envelope<M>(pub ActorId, pub M);

/// A clonable, typed handle to an actor's mailbox.
pub struct ActorRef<M: Send + 'static, R: BloxRuntime> {
    id: ActorId,
    tx: R::Sender<M>,
}

impl<M: Send + 'static, R: BloxRuntime> ActorRef<M, R> {
    /// Construct an `ActorRef` from a raw sender. Called by `StaticChannelCap::channel` or `DynamicChannelCap::channel`.
    pub fn new(id: ActorId, tx: R::Sender<M>) -> Self {
        Self { id, tx }
    }

    /// Returns the actor's unique identifier.
    pub fn id(&self) -> ActorId {
        self.id
    }

    /// Send a message, awaiting capacity if the mailbox is full.
    pub async fn send(&self, from: ActorId, payload: M) -> Result<(), R::SendError> {
        R::send_via(&self.tx, Envelope(from, payload)).await
    }

    /// Try to send without blocking. Returns an error if the mailbox is full.
    pub fn try_send(&self, from: ActorId, payload: M) -> Result<(), R::TrySendError> {
        R::try_send_via(&self.tx, Envelope(from, payload))
    }

    /// Returns a clone of the raw sender. Used by the wiring layer when a
    /// supervised actor needs to notify a supervisor directly via the raw
    /// sender type (e.g. to construct an `EmbassyChildHandle`).
    pub fn sender(&self) -> R::Sender<M> {
        self.tx.clone()
    }
}

impl<M: Send + 'static, R: BloxRuntime> Clone for ActorRef<M, R> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            tx: self.tx.clone(),
        }
    }
}

// SAFETY: The where-bounds guarantee that `R::Sender<M>` is Send/Sync at
// every instantiation site, so the containing `ActorRef` inherits those
// properties. Using where clauses instead of blanket `unsafe impl` lets
// the compiler verify the invariant at each concrete use rather than
// trusting a global assertion.
unsafe impl<M: Send + 'static, R: BloxRuntime> Send for ActorRef<M, R> where R::Sender<M>: Send {}
unsafe impl<M: Send + 'static, R: BloxRuntime> Sync for ActorRef<M, R> where R::Sender<M>: Sync {}
