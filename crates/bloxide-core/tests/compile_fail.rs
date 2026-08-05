// Copyright 2025 Bloxide, all rights reserved
//! Compile-fail tests for the statically wired actor-ID limit.
//!
//! `next_actor_id!` bakes a compile-time guard
//! (`const _: () = assert!(id < DYNAMIC_ACTOR_ID_BASE)`) into its expansion,
//! so allocating more than 255 static actors (`DYNAMIC_ACTOR_ID_BASE - 1`)
//! is a compile error, not a runtime collision. The proc-macro counter
//! starts at 1 per compilation, so a self-contained UI case can drive it
//! past the limit with 256 expansion sites.

#[test]
fn next_actor_id_past_static_limit_fails_to_compile() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/next_actor_id_overflow.rs");
}
