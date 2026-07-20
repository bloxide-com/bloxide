// Copyright 2025 Bloxide, all rights reserved
use crate::messaging::{ActorId, ActorRef, Envelope};
use core::future::Future;

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

    /// Kill capability. `NoKill` for static runtimes, `Kill` for dynamic.
    /// Determines the `Handle` type stored in `ChildEntry::abort_handle` —
    /// `()` (ZST) for `NoKill`, `R::AbortHandle` for `Kill`.
    ///
    /// Each runtime impl specifies this explicitly (no default — associated
    /// type defaults are unstable on stable Rust).
    type Kill: KillCapability<Self>;
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

/// Tier 2 capability for runtimes that support spawning actor tasks at runtime.
///
/// Extends `DynamicChannelCap` (which provides `alloc_actor_id` and `channel`).
/// Blox crates that need dynamic spawning declare `R: SpawnCap`.
/// Embassy does NOT implement this trait — use static wiring for Embassy.
///
/// The associated `TaskHandle` type is returned by `spawn` and is used to
/// produce an `AbortHandle` (the cloneable ripcord). For Tokio,
/// `TaskHandle = JoinHandle<()>` and `AbortHandle = tokio::task::AbortHandle`.
/// For a future Embassy task-pool runtime, `AbortHandle` would be `()` (no
/// external abort — the kill mailbox is sufficient) or whatever Embassy
/// provides if [issue #3197](https://github.com/embassy-rs/embassy/issues/3197)
/// is implemented.
///
/// All types are concrete, by-value — no `Arc<dyn>`, no dynamic dispatch.
pub trait SpawnCap: DynamicChannelCap {
    /// Handle to a spawned task. Used to derive an [`AbortHandle`](Self::AbortHandle).
    /// Consumed by [`abort_handle`](Self::abort_handle).
    type TaskHandle: Send + 'static;

    /// Cloneable handle for external task abort. Must be `Clone` so it can
    /// be extracted from `&Event` in action functions (the HSM engine passes
    /// `&Event`, not `&mut Event`). `()` for runtimes without external abort.
    type AbortHandle: Clone + Send + 'static;

    /// Spawn a future as an independent task and return a handle.
    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self::TaskHandle;

    /// Derive a cloneable abort handle from a task handle.
    /// The task handle is consumed; the task continues running (drop does not abort).
    fn abort_handle(handle: Self::TaskHandle) -> Self::AbortHandle;

    /// Abort a spawned task immediately via its abort handle. No callbacks fire —
    /// the task is dropped in-place. The handle is consumed and cannot be reused.
    fn abort(handle: Self::AbortHandle);
}

/// Type-level kill capability for a runtime.
///
/// `NoKill` — no external task abort (Embassy, static-only). `Handle = ()` (ZST).
/// `Kill`   — external abort via `SpawnCap::abort(handle)` (Tokio, dynamic).
///
/// This is a type-level enum, not a trait object. The runtime picks the
/// variant; the supervisor is monomorphized for whichever it is.
///
/// The `Handle` type is the cloneable `AbortHandle` from `SpawnCap`, NOT the
/// `TaskHandle`. This is because the handle must be `Clone` so it can be
/// extracted from `&Event` in action functions (the HSM engine passes `&Event`,
/// not `&mut Event`). The spawn function calls `SpawnCap::abort_handle()` to
/// convert the non-Clone `TaskHandle` into the Clone `AbortHandle` before
/// placing it in `RegisterDynamicChild`.
pub trait KillCapability<R: BloxRuntime> {
    type Handle: Clone + Send + 'static;
    fn kill(handle: Self::Handle);
}

/// No kill capability — static runtimes (Embassy). `Handle = ()` (ZST).
pub struct NoKill;
impl<R: BloxRuntime> KillCapability<R> for NoKill {
    type Handle = ();
    fn kill(_: ()) {}
}

/// Kill capability via `SpawnCap::abort`. Used by dynamic runtimes (Tokio).
pub struct Kill;
impl<R: BloxRuntime + SpawnCap> KillCapability<R> for Kill {
    type Handle = R::AbortHandle;
    fn kill(handle: R::AbortHandle) {
        R::abort(handle);
    }
}
