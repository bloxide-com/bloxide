// Copyright 2025 Bloxide, all rights reserved
use crate::messaging::{ActorId, ActorRef, Envelope};

/// Base trait for runtime-specific message sending and receiving.
///
/// This is the **only** trait that blox crates are generic over (`<R: BloxRuntime>`).
/// It abstracts the runtime-specific channel implementations while keeping
/// blox code runtime-agnostic.
///
/// # Associated Types
///
/// * `Sender<M>` — Send-side of a typed channel. Must be cheaply cloneable
///   (typically `Arc`-based) since `ActorRef<M>` clones it. The sender wraps
///   envelope metadata (sender ID) alongside the message payload.
/// * `Receiver<M>` — Receive-side of a typed channel. Converts to a `Stream<M>`
///   via [`to_stream`](Self::to_stream).
/// * `Stream<M>` — A fused stream that yields `Envelope<M>` values. The actor
///   run loop uses `futures::StreamExt::next` to await incoming messages.
/// * `SendError` — Error returned by [`send_via`](Self::send_via) when the
///   operation fails (e.g., channel closed). Most runtimes succeed unless
///   the receiver is dropped.
/// * `TrySendError` — Error returned specifically by [`try_send_via`](Self::try_send_via)
///   when the channel buffer is full (non-blocking send failed).
///
/// # When to Use Which Send Method
///
/// * **`send_via`** — Use when you want to wait for capacity (async). The sender
///   will await until space is available in the channel buffer. This is the
///   default choice for most actor-to-actor messaging.
/// * **`try_send_via`** — Use when you need non-blocking behavior. Returns
///   immediately with `Err(TrySendError)` if the channel is full. Useful for
///   implementing backpressure-aware protocols or bounded-mailbox actors.
///
/// # Converting Receivers to Streams
///
/// Use [`to_stream(receiver)`](Self::to_stream) to convert a `Receiver<M>` into a
/// `Stream<M>`. This is how the run loop receives messages via `futures::StreamExt::next`.
/// The stream yields `Envelope<M>` values containing both the message payload
/// and the sender's `ActorId`.
///
/// # Channel Creation
///
/// This trait does **not** include channel creation. See:
/// * [`StaticChannelCap`] — For compile-time-fixed capacity (Embassy, `no_std`)
/// * [`DynamicChannelCap`] — For runtime-configurable capacity (Tokio, `std`)
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
    ///
    /// This is an async operation that will wait until space is available
    /// in the channel buffer. Use this for normal actor-to-actor messaging
    /// where you want backpressure to flow naturally.
    async fn send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::SendError>;

    /// Try to send `envelope` via `sender` without blocking.
    ///
    /// Returns immediately:
    /// - `Ok(())` if the message was queued successfully
    /// - `Err(TrySendError)` if the channel buffer is full
    ///
    /// Use this for non-blocking sends where you want to implement custom
    /// backpressure handling or drop messages under load.
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

/// Capability to terminate actors and free allocated resources for dynamic actors based on policy.
///
/// Kill bypasses dispatch for immediate termination; used for policy-driven cleanup
/// or when actors are unresponsive.
///
/// Use `Arc<dyn KillCap>` for object-safe usage - the Arc provides clone functionality.
pub trait KillCap: Send + Sync + 'static {
    /// Kill the actor immediately. Runtime aborts the task and drops channels.
    /// No callbacks fire on the actor - it's just gone.
    fn kill(&self, actor_id: ActorId);
}
