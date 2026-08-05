// Copyright 2025 Bloxide, all rights reserved
//! Generate a complete binary `main.rs` from a `system.toml` wiring manifest.

mod actor_kind;
mod ctor_fields;
mod emit;
mod fmt;
mod paths;
mod payload;
mod validate;

pub use emit::generate;
pub(crate) use fmt::rustfmt_source;
pub use paths::extract_crates_from_path;
