// Copyright 2025 Bloxide, all rights reserved
//! Convention-based context derivation.
//!
//! This module implements the `#[derive(BloxCtx)]` macro with naming conventions
//! instead of explicit field annotations.
//!
//! # Conventions
//!
//! | Field Pattern | Inferred Role |
//! |---------------|----------------|
//! | `self_id: ActorId` | Auto-generates `impl HasSelfId` |
//! | `foo_ref: ActorRef<M, R>` | Auto-generates `impl HasFooRef<R>` if trait method is `fn foo_ref()` |
//! | `foo_factory: fn(...)` | Auto-generates `impl HasFooFactory<R>` if trait method is `fn foo_factory()` |
//! | `behavior: B` | Requires `#[delegates(Traits)]` for delegation |
//!
//! # Remaining Annotations
//!
//! - `#[delegates(Trait1, Trait2, ...)]` is required to mark behavior delegation fields.
//! - `#[provides(Trait<R>)]` is the canonical way to bind multi-param accessor traits
//!   (e.g. `HasPeerRef<R, PingPongMsg>`) that convention-based inference cannot infer.
//! - `#[blox_ctx(skip)]` suppresses auto-detection and makes the field a plain constructor
//!   parameter with no trait impl generated.
//!
//! # Backward Compatibility
//!
//! Old annotations `#[self_id]` and `#[ctor]` are no longer supported and produce
//! compile errors. Use naming conventions instead. Use `#[blox_ctx(skip)]` to suppress
//! auto-detection for fields that should be plain constructor parameters.

mod analyze;
mod generate;

use proc_macro2::TokenStream;
use syn::DeriveInput;

pub fn derive_blox_ctx_inner(input: &DeriveInput) -> syn::Result<TokenStream> {
    let analysis = analyze::analyze(input)?;
    generate::generate(&analysis)
}
