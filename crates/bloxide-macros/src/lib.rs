// Copyright 2025 Bloxide, all rights reserved
//! Procedural macros for bloxide.
//!
//! Provides ergonomic derive and attribute macros for blox authors.
//!
//! # Available macros
//!
//! Event enums (two complementary forms — `bloxide-codegen` generates plain
//! Rust from `blox.toml` and uses neither; these are for hand-written code):
//!
//! - `#[blox_event]` (attribute) — decorate an existing hand-written event
//!   enum whose variants each wrap one `Envelope<M>`; generates the `From`,
//!   `EventTag`, tag-constant, and accessor impls around it.
//! - `event!(Name { Variant: MsgType, ... })` (fn-like) — generate the whole
//!   event enum (including the `Lifecycle` variant) from a mailbox spec.
//!
//! Other macros:
//!
//! - `#[derive(EventTag)]` — assign sequential `u8` variant tags and `*_TAG`
//!   constants to any event enum.
//! - `blox_messages!(pub enum M { ... })` — generate message structs/enums.
//! - `channels!(RuntimeType; MsgType1(CAP1), ...)` — generate
//!   channel creation code via `StaticChannelCap`.
//! - `dyn_channels!(RuntimeType; MsgType1(CAP1), ...)` —
//!   generate channel creation code via `DynamicChannelCap`.
//! - `next_actor_id!()` — allocate the next compile-time actor ID from the
//!   same counter used by `channels!`.

use proc_macro::TokenStream;

mod blox_event;
mod channels;
mod dyn_channels;
mod event_tag;

mod blox_event_new;
mod blox_messages;

// ── EventTag derive ───────────────────────────────────────────────────────────

/// Derive [`EventTag`] for an event enum.
///
/// Assigns each variant a sequential `u8` tag (0, 1, 2, ...) by declaration
/// order. Also generates associated `VARIANT_TAG` constants in UPPER_SNAKE_CASE
/// so transition rules can reference them for fast pre-filtering.
///
/// Enums with more than 254 variants are rejected at compile time (255 is
/// reserved as the `WILDCARD_TAG` sentinel in `TransitionRule::event_tag`).
///
/// # Example
///
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// #[derive(EventTag, Debug)]
/// pub enum TEvent { GoB, GoC, Start }
/// // Generates:
/// // impl EventTag for TEvent { fn event_tag(&self) -> u8 { match self { Self::GoB => 0, ... } } }
/// // impl TEvent { pub const GO_B_TAG: u8 = 0; pub const GO_C_TAG: u8 = 1; pub const START_TAG: u8 = 2; }
/// ```
#[proc_macro_derive(EventTag)]
pub fn derive_event_tag(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    match event_tag::derive_event_tag_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

// ── #[blox_event] attribute ───────────────────────────────────────────────────

/// Generate boilerplate for a blox event enum.
///
/// Apply this attribute to an event enum whose variants each wrap exactly one
/// `Envelope<M>` value. The attribute generates:
/// - `From<Envelope<M>>` impl for each variant
/// - `EventTag` impl with sequential `u8` tags
/// - `VARIANT_TAG` constants in UPPER_SNAKE_CASE
/// - `variant_payload()` and `variant_envelope()` accessor methods
///
/// # Example
///
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// use bloxide_macros::blox_event;
///
/// #[blox_event]
/// #[derive(Debug)]
/// pub enum PingEvent {
///     Msg(Envelope<PingPongMsg>),
/// }
/// ```
///
/// Generates `From<Envelope<PingPongMsg>> for PingEvent`, `EventTag`,
/// `PingEvent::MSG_TAG`, `msg_payload()`, and `msg_envelope()`.
#[proc_macro_attribute]
pub fn blox_event(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemEnum);
    match blox_event::blox_event_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

// ── channels!(RuntimeType; MsgType1(CAP1), ...) ───────────────────────────────

/// Generate channel creation code for any number of mailboxes.
///
/// Syntax:
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// channels!(EmbassyRuntime; PingPongMsg(16), SomeMsg(8))
/// ```
///
/// Generates a block expression that returns `((ref1, ref2, ...), (stream1, stream2, ...))`:
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// {
///     let (r1, s1) = <EmbassyRuntime as ::bloxide_core::capability::StaticChannelCap>
///         ::channel::<PingPongMsg, 16>();
///     let (r2, s2) = <EmbassyRuntime as ::bloxide_core::capability::StaticChannelCap>
///         ::channel::<SomeMsg, 8>();
///     ((r1, r2,), (s1, s2,))
/// }
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
/// `channels!`. Returns a literal `usize` integer baked into generated code.
///
/// Useful for obtaining a supervisor's `ActorId` without a runtime atomic.
#[proc_macro]
pub fn next_actor_id(_input: TokenStream) -> TokenStream {
    use crate::channels::NEXT_ACTOR_ID;
    use core::sync::atomic::Ordering;
    let id = NEXT_ACTOR_ID.fetch_add(1, Ordering::Relaxed);
    quote::quote! { #id }.into()
}

// ── dyn_channels!(RuntimeType; MsgType1(CAP1), ...) ──────────────────────────

/// Generate channel creation code using `DynamicChannelCap` for runtimes with
/// runtime-configurable capacity (e.g. Tokio).
///
/// Syntax:
/// ```ignore
/// dyn_channels!(TokioRuntime; PingPongMsg(16), SomeMsg(8))
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

// ── blox_messages!(...) ──────────────────────────────────────────────────────

/// Generate message structs and enum from a declarative specification.
///
/// By default derives `Debug, Clone`. Prefix with `copy,` to also derive `Copy`:
///
/// ```ignore
/// // Without Copy (default — supports non-Copy field types like Vec, String)
/// blox_messages! {
///     pub enum WorkerMsg {
///         DoWork { payload: Vec<u8> },
///     }
/// }
///
/// // With Copy
/// blox_messages!(copy, pub enum PingPongMsg {
///     Ping { round: u32 },
///     Pong { round: u32 },
///     Resume {},
/// })
/// ```
#[proc_macro]
pub fn blox_messages(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as blox_messages::BloxMessagesInput);
    match blox_messages::blox_messages_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

// ── event!(Name { Mailbox: Type }) ───────────────────────────────────────────

/// Generate a complete blox event type from a mailbox specification.
///
/// # Syntax
///
/// ```ignore
/// // Single mailbox:
/// event!(Ping { Msg: PingPongMsg });
///
/// // Multi-mailbox with generics:
/// event!(Worker<R: BloxRuntime> {
///     Peer: PeerCtrl<WorkerMsg, R>,
///     Msg: WorkerMsg,
/// });
/// ```
#[proc_macro]
pub fn event(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as blox_event_new::BloxEventInput);
    match blox_event_new::blox_event_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
