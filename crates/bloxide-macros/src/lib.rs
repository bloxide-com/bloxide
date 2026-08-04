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
