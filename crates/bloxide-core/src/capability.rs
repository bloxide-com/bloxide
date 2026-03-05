use crate::messaging::{ActorId, ActorRef, Envelope};

/// Base trait for sending and receiving messages.
///
/// Used as the bound on blox crates (`R: BloxRuntime`). Does not include
/// channel creation — see [`StaticChannelCap`] and [`DynamicChannelCap`].
#[allow(async_fn_in_trait)]
pub trait BloxRuntime: Clone + Send + 'static {
    type SendError: core::fmt::Debug + Send + 'static;
    type TrySendError: core::fmt::Debug + Send + 'static;

    /// The raw sender half stored inside `ActorRef`. Must be cheaply clonable.
    type Sender<M: Send + 'static>: Clone + Send + Sync + 'static;

    type Receiver<M: Send + 'static>: Send + 'static;

    /// Stream of incoming envelopes. Requires `futures_core::Stream`.
    type Stream<M: Send + 'static>: futures_core::Stream<Item = Envelope<M>>
        + Unpin
        + Send
        + 'static;

    fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M>;

    /// Send `envelope` via `sender`, awaiting capacity.
    async fn send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::SendError>;

    /// Try to send `envelope` via `sender` without blocking.
    fn try_send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::TrySendError>;
}

/// Channel creation for runtimes with compile-time-fixed capacity.
///
/// Used by `no_std` / Embassy runtimes where `Channel<Mutex, T, N>` requires
/// `N` as a const generic. Only the wiring layer (e.g. the `channels!` macro)
/// calls this trait. Blox crates are never generic over `StaticChannelCap`.
pub trait StaticChannelCap: BloxRuntime {
    /// Create a new channel with capacity `N` baked in at compile time and
    /// the given `id` as the actor's identity.
    /// Returns an `ActorRef` (send handle) and a `Receiver` (stream source).
    fn channel<M: Send + 'static, const N: usize>(
        id: ActorId,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>);
}

/// Channel creation for runtimes with runtime-configurable capacity.
///
/// Used by `std` / Tokio runtimes where channel capacity can be set at
/// runtime. Only the wiring layer calls this trait. Blox crates are never
/// generic over `DynamicChannelCap`.
pub trait DynamicChannelCap: BloxRuntime {
    /// Allocate the next actor ID from the runtime's internal counter.
    fn alloc_actor_id() -> ActorId;

    /// Create a new channel with the given `id` and runtime `capacity`.
    /// Returns an `ActorRef` (send handle) and a `Receiver` (stream source).
    fn channel<M: Send + 'static>(
        id: ActorId,
        capacity: usize,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>);
}
