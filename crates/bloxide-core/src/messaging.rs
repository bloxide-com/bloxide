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
/// ```
/// use bloxide_core::messaging::Envelope;
/// # struct Pong {
/// #     round: u32,
/// # }
/// # enum PingMsg {
/// #     Pong(Pong),
/// # }
/// # enum PingEvent {
/// #     Msg(Envelope<PingMsg>),
/// # }
///
/// // Ignore the sender (common case):
/// match PingEvent::Msg(Envelope(1, PingMsg::Pong(Pong { round: 7 }))) {
///     PingEvent::Msg(Envelope(_, PingMsg::Pong(Pong { round }))) => assert_eq!(round, 7),
/// }
///
/// // Match on sender when needed:
/// match PingEvent::Msg(Envelope(2, PingMsg::Pong(Pong { round: 8 }))) {
///     PingEvent::Msg(Envelope(from, PingMsg::Pong(Pong { round }))) => {
///         assert_eq!(from, 2);
///         assert_eq!(round, 8);
///     }
/// }
/// ```
#[derive(Debug, Clone)]
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
    /// sender type (e.g. to build a `RunConfig`'s `supervisor_notify`).
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

impl<M, R: BloxRuntime> core::fmt::Debug for ActorRef<M, R>
where
    M: Send + 'static,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ActorRef")
            .field("id", &self.id)
            .field("msg_type", &core::any::type_name::<M>())
            .finish()
    }
}

// Note: no manual `Send`/`Sync` impls. `BloxRuntime` already bounds
// `Sender<M>: Clone + Send + Sync` (capability.rs) and `ActorId` is `usize`,
// so the auto implementations apply — `ActorRef<M, R>` is `Send + Sync`
// wherever `R::Sender<M>` is, with no `unsafe` required.
