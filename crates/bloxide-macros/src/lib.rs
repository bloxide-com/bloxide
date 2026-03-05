// Copyright 2025 Bloxide, all rights reserved
//! Procedural macros for bloxide.
//!
//! Provides ergonomic derive and attribute macros that complement the
//! declarative macros in `bloxide-core`.
//!
//! # Available macros
//!
//! - `#[derive(BloxCtx)]` — derive accessor trait impls and a `fn new(...)`
//!   constructor for a blox context struct.
//!
//! - `#[derive(StateTopology)]` — derive `StateTopology` for a state enum.
//!
//! - `#[derive(EventTag)]` — derive `EventTag` for an event enum.
//!
//! - `#[blox_event]` — generate `From<Envelope<M>>` impls, `EventTag` impl,
//!   tag constants, and payload accessors for every variant of a blox event enum.
//!
//! - `mailboxes_impls!(N)` — generate `impl Mailboxes<E> for (S1, ..., SK)`
//!   for every arity `k` from 1 to N. Removes the hard tuple-arity cap.
//!
//! - `channels!(RuntimeType; MsgType1(CAP1), MsgType2(CAP2), ...)` — generate
//!   channel creation code for any number of mailboxes via `StaticChannelCap`.
//!
//! - `transitions!(ARMS)` / `root_transitions!(ARMS)` — build typed transition
//!   rule slices with automatic event-tag extraction.
//!
//! - `#[delegatable]` — keep a trait definition unchanged and emit a companion
//!   `macro_rules! __delegate_TraitName` macro that generates forwarding impls.

use proc_macro::TokenStream;

mod blox_ctx;
mod blox_event;
mod channels;
mod delegatable;
mod dyn_channels;
mod event_tag;
mod mailboxes_impls;
mod state_topology;
mod transitions;

// ── BloxCtx derive ────────────────────────────────────────────────────────────

/// Derive accessor trait impls and a `fn new(...)` constructor for a blox context struct.
///
/// # Supported field annotations
///
/// - `#[self_id]` — generates `impl HasSelfId for Struct { fn self_id(&self) -> ActorId }`
/// - `#[provides(TraitName<R>)]` — generates `impl TraitName<R> for Struct` with a
///   single accessor method (method name = field name, return type = `&FieldType`)
/// - `#[ctor]` — marks the field as a constructor parameter without generating any
///   trait impl; useful for fields like `ActorRef` or factory functions that are
///   passed in at construction but do not have a single-accessor trait
/// - `#[delegates(TraitName)]` — emits `__delegate_TraitName!(...)` companion macro
///   invocations that generate forwarding impls (the trait must be annotated with
///   `#[delegatable]` from this crate)
///
/// # Constructor
///
/// `fn new(...)` takes annotated fields as parameters and zero-initializes
/// unannotated fields via `Default::default()`.
///
/// # Example
///
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// #[derive(BloxCtx)]
/// pub struct PingCtx<R: BloxRuntime> {
///     #[self_id]
///     pub self_id: ActorId,
///     #[provides(HasPeerRef<R>)]
///     pub peer_ref: ActorRef<PingPongMsg, R>,
///     #[provides(HasTimerRef<R>)]
///     pub timer_ref: ActorRef<TimerCommand, R>,
///     pub round: u32,           // → Default::default() in constructor
/// }
/// ```
///
/// Generates:
/// - `impl HasSelfId for PingCtx<R>`
/// - `impl HasPeerRef<R> for PingCtx<R> { fn peer_ref(&self) -> &ActorRef<...> }`
/// - `impl HasTimerRef<R> for PingCtx<R> { fn timer_ref(&self) -> &ActorRef<...> }`
/// - `fn new(self_id, peer_ref, timer_ref) -> Self { round: 0, ... }`
#[proc_macro_derive(BloxCtx, attributes(self_id, ctor, provides, delegates))]
pub fn derive_blox_ctx(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    match blox_ctx::derive_blox_ctx_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

// ── StateTopology derive ──────────────────────────────────────────────────────

/// Derive [`StateTopology`] for a state enum.
///
/// Requires `#[repr(u8)]` on the enum. Each variant must be a unit variant.
///
/// Use the following attributes on variants:
/// - `#[composite]` — marks a state as having children (not a leaf).
///   Composite states may not be transition targets.
/// - `#[parent(ParentVariant)]` — declares the parent state for non-top-level
///   states. Top-level states (no parent) require no attribute.
///
/// # Example
///
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// #[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
/// #[repr(u8)]
/// pub enum PingState {
///     #[composite]
///     Operating,
///     #[parent(Operating)]
///     Active,
///     #[parent(Operating)]
///     Paused,
///     Done,
/// }
/// ```
///
/// Generates `impl StateTopology for PingState` with:
/// - `STATE_COUNT = 4`
/// - `parent()` returning the declared parent, or `None` for top-level
/// - `is_leaf()` returning `true` for non-composite states
/// - `path()` returning a statically-allocated root-first ancestor slice
/// - `as_index()` returning the `repr(u8)` discriminant as `usize`
#[proc_macro_derive(StateTopology, attributes(composite, parent, handler_fns))]
pub fn derive_state_topology(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    match state_topology::derive_state_topology_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

// ── EventTag derive ───────────────────────────────────────────────────────────

/// Derive [`EventTag`] for an event enum.
///
/// Assigns each variant a sequential `u8` tag (0, 1, 2, ...) by declaration
/// order. Also generates associated `VARIANT_TAG` constants in UPPER_SNAKE_CASE
/// so transition rules can reference them for fast pre-filtering.
///
/// Enums with more than 254 variants are rejected at compile time (255 is
/// reserved as [`WILDCARD_TAG`] in `TransitionRule::event_tag`).
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
///
/// [`WILDCARD_TAG`]: bloxide_core::event_tag::WILDCARD_TAG
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

// ── mailboxes_impls!(N) ───────────────────────────────────────────────────────

/// Generate `impl Mailboxes<E> for (S1, ..., SK)` for every arity `k` from 1 to N.
///
/// Call this once in `mailboxes.rs` to replace the hard-coded 4-tuple limit:
///
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// bloxide_macros::mailboxes_impls!(16);
/// ```
///
/// The generated impls mirror the hand-written ones exactly: each stream is
/// polled in index order (priority order), `Poll::Ready(None)` triggers a
/// `debug_assert!` (self-sender invariant violation), and `Poll::Pending` falls
/// through to the next stream.
#[proc_macro]
pub fn mailboxes_impls(input: TokenStream) -> TokenStream {
    mailboxes_impls::mailboxes_impls_inner(input)
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
/// is a runtime `usize` value and `id` is allocated via `alloc_actor_id()`.
///
/// Returns `((ref1, ref2, ...), (stream1, stream2, ...))`.
///
/// Typically wrapped by a runtime-specific thin macro (e.g.
/// `bloxide_tokio::channels!`) that hard-codes the runtime type.
#[proc_macro]
pub fn dyn_channels(input: TokenStream) -> TokenStream {
    dyn_channels::dyn_channels_inner(input)
}

// ── transitions!(ARMS) and root_transitions!(ARMS) ────────────────────────────

/// Build a `&'static [StateRule<S>]` from transition rule arms.
///
/// This proc-macro version automatically extracts the `event_tag` from
/// each arm's pattern, enabling the engine to skip non-matching rules
/// without a function pointer call.
#[proc_macro]
pub fn transitions(input: TokenStream) -> TokenStream {
    transitions::transitions_inner(input, false)
}

/// Build a `&'static [StateRule<S>]` from root-level transition rule arms.
///
/// Root rules use the same `StateRule<S>` type as state-level rules —
/// `root_transitions()` returns `&'static [StateRule<Self>]`. There is no
/// separate `RootRule` type; both macros generate identical `StateRule` items.
///
/// This proc-macro version automatically extracts the `event_tag` from
/// each arm's pattern, enabling the engine to skip non-matching rules
/// without a function pointer call.
#[proc_macro]
pub fn root_transitions(input: TokenStream) -> TokenStream {
    transitions::transitions_inner(input, true)
}

// ── #[delegatable] attribute ────────────────────────────────────────────────

/// Keep a trait definition unchanged and generate a companion
/// `macro_rules! __delegate_TraitName` macro.
///
/// The generated macro accepts struct/field/generics parameters and produces
/// a forwarding `impl TraitName for Struct` that delegates every associated
/// type and method to a named field.
///
/// # Example
///
/// ```ignore
/// // Doc test ignored: imports not resolvable in rustdoc compilation context
/// use bloxide_macros::delegatable;
///
/// #[delegatable]
/// pub trait CountsRounds {
///     type Round: Copy;
///     fn round(&self) -> Self::Round;
///     fn set_round(&mut self, round: Self::Round);
/// }
/// ```
///
/// Generates `__delegate_CountsRounds!` which, when invoked, emits:
/// ```ignore
/// impl<...> CountsRounds for MyStruct<...>
/// where FieldType: CountsRounds, ...
/// {
///     type Round = <FieldType as CountsRounds>::Round;
///     fn round(&self) -> Self::Round { self.field.round() }
///     fn set_round(&mut self, round: Self::Round) { self.field.set_round(round) }
/// }
/// ```
///
/// # Limitations
///
/// The trait must not have generic type parameters. Associated types are supported.
/// Applying `#[delegatable]` to a trait with type parameters will produce a compilation
/// error from the generated macro.
#[proc_macro_attribute]
pub fn delegatable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    delegatable::delegatable_inner(item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
