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

    /// Kill capability. `NoKill` for static runtimes, `Kill` for dynamic.
    /// Determines the `Handle` type stored in `ChildEntry::kill_handle` —
    /// `()` (ZST) for `NoKill`, `R::KillHandle` for `Kill`.
    ///
    /// Each runtime impl specifies this explicitly (no default — associated
    /// type defaults are unstable on stable Rust).
    type Kill: KillCapability<Self>;

    /// Cooperative yield to the executor.
    ///
    /// Called by the run loop after processing each message to give other
    /// tasks a chance to run. Default is a no-op — runtimes with cooperative
    /// schedulers (Tokio, Embassy) override this to call their runtime's
    /// `yield_now()`. TestRuntime and bare runtimes use the default.
    async fn yield_now() {
        // No-op default
    }
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

/// First actor ID available for runtime allocation (dynamic spawning).
///
/// Compile-time allocation (`channels!`, `dyn_channels!`, `next_actor_id!`)
/// hands out small sequential IDs starting at 1, so the static ID space is
/// `1..DYNAMIC_ACTOR_ID_BASE` — a hard limit of 255 statically wired actors
/// per system. The wiring macros bake a compile-time guard
/// (`const _: () = assert!(id < DYNAMIC_ACTOR_ID_BASE)`) into their expansion,
/// so exceeding the limit is a compile error, not a runtime collision.
///
/// Runtime `alloc_actor_id` counters MUST start at this base so dynamically
/// spawned actors can never collide with compile-time IDs.
pub const DYNAMIC_ACTOR_ID_BASE: usize = 256;

/// Channel creation for runtimes with runtime-configurable capacity.
///
/// Used by `std` / Tokio runtimes where channel capacity can be set at
/// runtime. Only the wiring layer calls this trait. Blox crates are never
/// generic over `DynamicChannelCap`.
pub trait DynamicChannelCap: BloxRuntime {
    /// Allocate the next actor ID from the runtime's dynamic counter, starting
    /// at [`DYNAMIC_ACTOR_ID_BASE`]. Used by dynamically spawned actors;
    /// static wiring uses the compile-time counter instead.
    fn alloc_actor_id() -> ActorId;

    /// Create a new channel with the given `id` and runtime `capacity`.
    /// Returns an `ActorRef` (send handle) and a `Receiver` (stream source).
    fn channel<M: Send + 'static>(
        id: ActorId,
        capacity: usize,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>);
}

/// Channel creation for supervision child groups, uniform across runtimes.
///
/// Lets a single `ChildGroupBuilder` (bloxide-child-management) serve every
/// runtime: dynamic-channel runtimes (Tokio, TestRuntime) forward to
/// [`DynamicChannelCap`]; static runtimes (Embassy) forward to
/// [`StaticChannelCap`]. Only the wiring layer (the group builder) calls
/// this trait. Blox crates are never generic over `GroupChannelCap`.
pub trait GroupChannelCap: BloxRuntime {
    /// Allocate an actor ID for a group channel (notify/control).
    ///
    /// Dynamic runtimes forward to their runtime counter (distinct ID per
    /// call). Static runtimes bake a compile-time ID per expansion site —
    /// the notify and control channels are both mailboxes of the one
    /// logical group actor.
    fn alloc_group_id() -> ActorId;

    /// Create a group channel with capacity `N` and the given `id`.
    /// Dynamic runtimes pass `N` as the runtime capacity; static runtimes
    /// bake it as a const generic. Returns an `ActorRef` (send handle) and
    /// a `Receiver` (stream source).
    fn group_channel<M: Send + 'static, const N: usize>(
        id: ActorId,
    ) -> (ActorRef<M, Self>, Self::Receiver<M>);
}

/// Type-level kill capability for a runtime.
///
/// `NoKill` — no external task kill (Embassy, static-only). `Handle = ()` (ZST).
/// `Kill`   — external kill via `SpawnCap::kill(handle)` (Tokio, dynamic).
///         `Kill` lives in `bloxide-spawn`, not here, because it requires the
///         `SpawnCap` bound.
///
/// This is a type-level enum, not a trait object. The runtime picks the
/// variant; the supervisor is monomorphized for whichever it is.
///
/// The `Handle` type is the cloneable `KillHandle` from `SpawnCap`, NOT the
/// `TaskHandle`. This is because the handle must be `Clone` so it can be
/// extracted from `&Event` in action functions (the HSM engine passes `&Event`,
/// not `&mut Event`). The spawn function calls `SpawnCap::kill_handle()` to
/// convert the non-Clone `TaskHandle` into the Clone `KillHandle` before
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

#[cfg(test)]
mod tests {
    use super::DYNAMIC_ACTOR_ID_BASE;

    /// Pins the actor-ID split: compile-time allocation (`channels!`,
    /// `dyn_channels!`, `next_actor_id!`) starts at 1, so the static space is
    /// `1..DYNAMIC_ACTOR_ID_BASE` — a hard limit of 255 statically wired
    /// actors per system; dynamic IDs start at the base.
    ///
    /// The compile-fail side of the wiring-macro guard (an expansion baking
    /// an ID ≥ 256 fails the const assert) is not testable in-tree:
    /// `trybuild` is not a dependency and proc-macro counter state cannot be
    /// forced to the limit from a unit test. The pass side is covered by
    /// every workspace build — all existing `channels!` / `dyn_channels!` /
    /// `next_actor_id!` expansions carry the guard and compile.
    #[test]
    fn dynamic_actor_id_base_caps_static_space_at_255() {
        assert_eq!(DYNAMIC_ACTOR_ID_BASE, 256);
        // Compile-time counter starts at 1 (bloxide-macros `NEXT_ACTOR_ID`).
        assert_eq!(DYNAMIC_ACTOR_ID_BASE - 1, 255);
    }
}
