// Copyright 2025 Bloxide, all rights reserved
//! Generate state topology enum and `StateTopology` implementation.
//! When declarative transitions are present in the TOML, also generates
//! complete `StateFns` constants with raw `StateRule { ... }` struct literals.

mod elision;
mod emit;
mod patterns;
mod rules;

pub(crate) use elision::catchall_elision;
pub use emit::generate;
pub(crate) use rules::generate_state_rule;
