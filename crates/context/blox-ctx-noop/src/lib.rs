// Copyright 2025 Bloxide, all rights reserved
//! Shared no-op context crate — the composable context-crate pattern
//! (spec 13) demonstrated at its smallest: one crate, one action function,
//! any number of consumers.
//!
//! Demo and test bloxes whose actions are intentional no-ops compose this
//! crate instead of declaring per-blox stub functions — e.g.
//! `crate = "blox_ctx_noop"` + `fn_name = "noop"` in a `[[context.actions]]`
//! entry. First consumer: the bhsm-tst topology blox.
#![no_std]

use bloxide_core::transition::ActionResult;

/// Do nothing and report success.
///
/// Takes no context fields, so the system codegen can wire it into every
/// action slot:
///
/// - **Entry/exit** (`fn(&mut Ctx)`, infallible): the generated closure is
///   `|ctx| { ::blox_ctx_noop::noop(); }` — the return value is discarded.
/// - **Transition** (`fn(&mut Ctx, &Event) -> ActionResult`): declare
///   `returns = "ActionResult"` on the action entry so the generated closure
///   emits the call bare, `|ctx, _ev| { ::blox_ctx_noop::noop() }`, with no
///   `ActionResult::from(...)` normalization wrapper.
pub fn noop() -> ActionResult {
    ActionResult::Ok
}
