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
//! # Remaining Annotation
//!
//! Only `#[delegates(Trait1, Trait2, ...)]` is required to mark behavior delegation fields.
//!
//! # Backward Compatibility
//!
//! Old annotations (`#[self_id]`, `#[provides]`, `#[ctor]`) are deprecated but still work.
//! They emit compile-time warnings.

mod analyze;
mod generate;

use proc_macro2::TokenStream;
use syn::DeriveInput;

pub fn derive_blox_ctx_inner(input: &DeriveInput) -> syn::Result<TokenStream> {
    let analysis = analyze::analyze(input)?;
    generate::generate(&analysis)
}
