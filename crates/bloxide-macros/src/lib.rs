// Copyright 2025 Bloxide, all rights reserved
//! Procedural macros for bloxide.
//!
//! `bloxide-codegen` generates all blox boilerplate (event enums, message
//! types, handler tables) from `blox.toml`, so the remaining macro surface is
//! only the channel/ID helpers used by runtime wiring code:
//!
//! - `channels!(RuntimeType; MsgType1(CAP1), ...)` — generate
//!   channel creation code via `StaticChannelCap`.
//! - `dyn_channels!(RuntimeType; MsgType1(CAP1), ...)` —
//!   generate channel creation code via `DynamicChannelCap`.
//! - `next_actor_id!()` — allocate the next compile-time actor ID from the
//!   same counter used by `channels!`.

use proc_macro::TokenStream;

mod channels;
mod dyn_channels;

// ── channels!(RuntimeType; MsgType1(CAP1), ...) ───────────────────────────────

/// Generate channel creation code for any number of mailboxes.
///
/// Syntax:
/// ```
/// use bloxide_macros::channels;
/// # use bloxide_core::capability::{BloxRuntime, NoKill, StaticChannelCap};
/// # use bloxide_core::messaging::{ActorId, ActorRef, Envelope};
/// # use std::collections::VecDeque;
/// # use std::sync::{Arc, Mutex};
/// #
/// # // Minimal in-memory BloxRuntime so the example can compile and run.
/// # #[derive(Clone)]
/// # struct MockRuntime;
/// #
/// # // One shared queue backs the sender, receiver, and stream of a channel.
/// # struct Chan<M: Send + 'static>(Arc<Mutex<VecDeque<Envelope<M>>>>);
/// # impl<M: Send + 'static> Clone for Chan<M> {
/// #     fn clone(&self) -> Self {
/// #         Self(Arc::clone(&self.0))
/// #     }
/// # }
/// # impl<M: Send + 'static> futures_core::Stream for Chan<M> {
/// #     type Item = Envelope<M>;
/// #     fn poll_next(
/// #         self: core::pin::Pin<&mut Self>,
/// #         _: &mut core::task::Context<'_>,
/// #     ) -> core::task::Poll<Option<Self::Item>> {
/// #         core::task::Poll::Ready(self.0.lock().unwrap().pop_front())
/// #     }
/// # }
/// #
/// # impl BloxRuntime for MockRuntime {
/// #     type SendError = ();
/// #     type TrySendError = ();
/// #     type Sender<M: Send + 'static> = Chan<M>;
/// #     type Receiver<M: Send + 'static> = Chan<M>;
/// #     type Stream<M: Send + 'static> = Chan<M>;
/// #     type Kill = NoKill;
/// #     fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M> {
/// #         rx
/// #     }
/// #     async fn send_via<M: Send + 'static>(
/// #         tx: &Self::Sender<M>,
/// #         env: Envelope<M>,
/// #     ) -> Result<(), Self::SendError> {
/// #         tx.0.lock().unwrap().push_back(env);
/// #         Ok(())
/// #     }
/// #     fn try_send_via<M: Send + 'static>(
/// #         tx: &Self::Sender<M>,
/// #         env: Envelope<M>,
/// #     ) -> Result<(), Self::TrySendError> {
/// #         tx.0.lock().unwrap().push_back(env);
/// #         Ok(())
/// #     }
/// #     fn try_send_error_is_closed(_: &Self::TrySendError) -> bool {
/// #         false
/// #     }
/// # }
/// #
/// # impl StaticChannelCap for MockRuntime {
/// #     fn channel<M: Send + 'static, const N: usize>(
/// #         id: ActorId,
/// #     ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
/// #         let chan = Chan(Arc::new(Mutex::new(VecDeque::new())));
/// #         (ActorRef::new(id, chan.clone()), chan)
/// #     }
/// # }
/// #
/// # enum PingPongMsg {
/// #     Ping,
/// # }
/// # enum SomeMsg {
/// #     Go,
/// # }
///
/// let ((ping_ref, some_ref), (_ping_stream, _some_stream)) =
///     channels!(MockRuntime; PingPongMsg(16), SomeMsg(8));
///
/// // All mailboxes from one `channels!` call belong to the same actor and
/// // share one compile-time actor ID.
/// assert_eq!(ping_ref.id(), some_ref.id());
///
/// // The refs are live typed send handles.
/// ping_ref.try_send(ping_ref.id(), PingPongMsg::Ping).unwrap();
/// some_ref.try_send(some_ref.id(), SomeMsg::Go).unwrap();
/// ```
///
/// Generates a block expression that returns `((ref1, ref2, ...), (stream1, stream2, ...))`.
/// Shown with `MockRuntime` standing in for the real runtime type and actor ID
/// `1` — the value the compile-time counter bakes into the first expansion:
/// ```
/// # use bloxide_core::capability::{BloxRuntime, NoKill, StaticChannelCap};
/// # use bloxide_core::messaging::{ActorId, ActorRef, Envelope};
/// # use std::collections::VecDeque;
/// # use std::sync::{Arc, Mutex};
/// #
/// # // Minimal in-memory BloxRuntime so the example can compile and run.
/// # #[derive(Clone)]
/// # struct MockRuntime;
/// #
/// # // One shared queue backs the sender, receiver, and stream of a channel.
/// # struct Chan<M: Send + 'static>(Arc<Mutex<VecDeque<Envelope<M>>>>);
/// # impl<M: Send + 'static> Clone for Chan<M> {
/// #     fn clone(&self) -> Self {
/// #         Self(Arc::clone(&self.0))
/// #     }
/// # }
/// # impl<M: Send + 'static> futures_core::Stream for Chan<M> {
/// #     type Item = Envelope<M>;
/// #     fn poll_next(
/// #         self: core::pin::Pin<&mut Self>,
/// #         _: &mut core::task::Context<'_>,
/// #     ) -> core::task::Poll<Option<Self::Item>> {
/// #         core::task::Poll::Ready(self.0.lock().unwrap().pop_front())
/// #     }
/// # }
/// #
/// # impl BloxRuntime for MockRuntime {
/// #     type SendError = ();
/// #     type TrySendError = ();
/// #     type Sender<M: Send + 'static> = Chan<M>;
/// #     type Receiver<M: Send + 'static> = Chan<M>;
/// #     type Stream<M: Send + 'static> = Chan<M>;
/// #     type Kill = NoKill;
/// #     fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M> {
/// #         rx
/// #     }
/// #     async fn send_via<M: Send + 'static>(
/// #         tx: &Self::Sender<M>,
/// #         env: Envelope<M>,
/// #     ) -> Result<(), Self::SendError> {
/// #         tx.0.lock().unwrap().push_back(env);
/// #         Ok(())
/// #     }
/// #     fn try_send_via<M: Send + 'static>(
/// #         tx: &Self::Sender<M>,
/// #         env: Envelope<M>,
/// #     ) -> Result<(), Self::TrySendError> {
/// #         tx.0.lock().unwrap().push_back(env);
/// #         Ok(())
/// #     }
/// #     fn try_send_error_is_closed(_: &Self::TrySendError) -> bool {
/// #         false
/// #     }
/// # }
/// #
/// # impl StaticChannelCap for MockRuntime {
/// #     fn channel<M: Send + 'static, const N: usize>(
/// #         id: ActorId,
/// #     ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
/// #         let chan = Chan(Arc::new(Mutex::new(VecDeque::new())));
/// #         (ActorRef::new(id, chan.clone()), chan)
/// #     }
/// # }
/// #
/// # enum PingPongMsg {
/// #     Ping,
/// # }
/// # enum SomeMsg {
/// #     Go,
/// # }
/// let ((r1, r2), (_s1, _s2)) = {
///     // Compile-time guard: statically wired actor IDs live below
///     // `DYNAMIC_ACTOR_ID_BASE`; exceeding the limit fails compilation here.
///     const _: () = assert!(
///         1 < ::bloxide_core::capability::DYNAMIC_ACTOR_ID_BASE,
///         "statically wired actor limit exceeded: compile-time actor IDs must stay below DYNAMIC_ACTOR_ID_BASE"
///     );
///     let (r1, s1) = <MockRuntime as ::bloxide_core::capability::StaticChannelCap>
///         ::channel::<PingPongMsg, 16>(1);
///     let (r2, s2) = <MockRuntime as ::bloxide_core::capability::StaticChannelCap>
///         ::channel::<SomeMsg, 8>(1);
///     ((r1, r2,), (s1, s2,))
/// };
///
/// assert_eq!(r1.id(), 1);
/// assert_eq!(r2.id(), 1);
/// ```
///
/// This macro is typically wrapped by a runtime-specific thin macro (e.g.
/// `bloxide_embassy::channels!`) that hard-codes the runtime type so call
/// sites don't need to pass it.
#[proc_macro]
pub fn channels(input: TokenStream) -> TokenStream {
    channels::channels_inner(input)
}

// ── next_actor_id!() ──────────────────────────────────────────────────────────

/// Allocate the next compile-time actor ID from the same counter used by
/// `channels!`. Expands to a block that evaluates to a literal `usize`
/// integer baked into generated code, preceded by a compile-time guard
/// (`const _: () = assert!(id < DYNAMIC_ACTOR_ID_BASE)`) so the 255-actor
/// static-wiring limit is enforced at compile time.
///
/// Useful for obtaining a supervisor's `ActorId` without a runtime atomic.
#[proc_macro]
pub fn next_actor_id(_input: TokenStream) -> TokenStream {
    use crate::channels::NEXT_ACTOR_ID;
    use core::sync::atomic::Ordering;
    let id = NEXT_ACTOR_ID.fetch_add(1, Ordering::Relaxed);
    quote::quote! {
        {
            const _: () = assert!(
                #id < ::bloxide_core::capability::DYNAMIC_ACTOR_ID_BASE,
                "statically wired actor limit exceeded: compile-time actor IDs must stay below DYNAMIC_ACTOR_ID_BASE"
            );
            #id
        }
    }
    .into()
}

// ── dyn_channels!(RuntimeType; MsgType1(CAP1), ...) ──────────────────────────

/// Generate channel creation code using `DynamicChannelCap` for runtimes with
/// runtime-configurable capacity (e.g. Tokio).
///
/// Syntax:
/// ```
/// use bloxide_macros::dyn_channels;
/// # use bloxide_core::capability::{
/// #     BloxRuntime, DynamicChannelCap, NoKill, DYNAMIC_ACTOR_ID_BASE,
/// # };
/// # use bloxide_core::messaging::{ActorId, ActorRef, Envelope};
/// # use std::collections::VecDeque;
/// # use std::sync::{Arc, Mutex};
/// #
/// # // Minimal in-memory BloxRuntime so the example can compile and run.
/// # #[derive(Clone)]
/// # struct MockRuntime;
/// #
/// # // One shared queue backs the sender, receiver, and stream of a channel.
/// # struct Chan<M: Send + 'static>(Arc<Mutex<VecDeque<Envelope<M>>>>);
/// # impl<M: Send + 'static> Clone for Chan<M> {
/// #     fn clone(&self) -> Self {
/// #         Self(Arc::clone(&self.0))
/// #     }
/// # }
/// # impl<M: Send + 'static> futures_core::Stream for Chan<M> {
/// #     type Item = Envelope<M>;
/// #     fn poll_next(
/// #         self: core::pin::Pin<&mut Self>,
/// #         _: &mut core::task::Context<'_>,
/// #     ) -> core::task::Poll<Option<Self::Item>> {
/// #         core::task::Poll::Ready(self.0.lock().unwrap().pop_front())
/// #     }
/// # }
/// #
/// # impl BloxRuntime for MockRuntime {
/// #     type SendError = ();
/// #     type TrySendError = ();
/// #     type Sender<M: Send + 'static> = Chan<M>;
/// #     type Receiver<M: Send + 'static> = Chan<M>;
/// #     type Stream<M: Send + 'static> = Chan<M>;
/// #     type Kill = NoKill;
/// #     fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M> {
/// #         rx
/// #     }
/// #     async fn send_via<M: Send + 'static>(
/// #         tx: &Self::Sender<M>,
/// #         env: Envelope<M>,
/// #     ) -> Result<(), Self::SendError> {
/// #         tx.0.lock().unwrap().push_back(env);
/// #         Ok(())
/// #     }
/// #     fn try_send_via<M: Send + 'static>(
/// #         tx: &Self::Sender<M>,
/// #         env: Envelope<M>,
/// #     ) -> Result<(), Self::TrySendError> {
/// #         tx.0.lock().unwrap().push_back(env);
/// #         Ok(())
/// #     }
/// #     fn try_send_error_is_closed(_: &Self::TrySendError) -> bool {
/// #         false
/// #     }
/// # }
/// #
/// # impl DynamicChannelCap for MockRuntime {
/// #     fn alloc_actor_id() -> ActorId {
/// #         DYNAMIC_ACTOR_ID_BASE
/// #     }
/// #     fn channel<M: Send + 'static>(
/// #         id: ActorId,
/// #         _capacity: usize,
/// #     ) -> (ActorRef<M, Self>, Self::Receiver<M>) {
/// #         let chan = Chan(Arc::new(Mutex::new(VecDeque::new())));
/// #         (ActorRef::new(id, chan.clone()), chan)
/// #     }
/// # }
/// #
/// # enum PingPongMsg {
/// #     Ping,
/// # }
/// # enum SomeMsg {
/// #     Go,
/// # }
///
/// let ((ping_ref, some_ref), (_ping_stream, _some_stream)) =
///     dyn_channels!(MockRuntime; PingPongMsg(16), SomeMsg(8));
///
/// // All mailboxes from one `dyn_channels!` call belong to the same actor and
/// // share one compile-time actor ID; the runtime's dynamic counter is not
/// // touched by static wiring.
/// assert_eq!(ping_ref.id(), some_ref.id());
///
/// // The refs are live typed send handles.
/// ping_ref.try_send(ping_ref.id(), PingPongMsg::Ping).unwrap();
/// some_ref.try_send(some_ref.id(), SomeMsg::Go).unwrap();
/// ```
///
/// Unlike `channels!` (which uses `StaticChannelCap` with a const-generic `N`),
/// this macro calls `DynamicChannelCap::channel(id, capacity)` where capacity
/// is a runtime `usize` value and `id` is baked from the same compile-time
/// counter used by `channels!` and `next_actor_id!`.
///
/// Returns `((ref1, ref2, ...), (stream1, stream2, ...))`.
///
/// Typically wrapped by a runtime-specific thin macro (e.g.
/// `bloxide_tokio::channels!`) that hard-codes the runtime type.
#[proc_macro]
pub fn dyn_channels(input: TokenStream) -> TokenStream {
    dyn_channels::dyn_channels_inner(input)
}
