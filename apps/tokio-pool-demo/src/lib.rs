// Copyright 2025 Bloxide, all rights reserved
//! Library target for `tokio-pool-demo`.
//!
//! Exposes the system-generated concrete specs (`src/generated/`, produced by
//! `cargo blox generate` from `system.toml`) so the integration tests in
//! `tests/` can drive the real machines — unlike the blox-crate-level specs,
//! whose action closures are stubs. The binary (`src/main.rs`) keeps its own
//! private `mod generated;`; this module exists only to make the specs
//! importable from tests.

pub mod generated;
