// Copyright 2025 Bloxide, all rights reserved
//! Semantic process exit codes (spec 19).
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0 | success |
//! | 1 | unspecified error |
//! | 2 | usage error (handled natively by clap) |
//! | 3 | not found (blox, crate, state, variant, actor, …) |
//! | 5 | conflict (entity already exists) |
//!
//! Commands construct [`not_found`] / [`conflict`] errors; `main` maps them
//! to the process exit code via [`exit_process`].

use std::fmt;

pub const EXIT_NOT_FOUND: i32 = 3;
pub const EXIT_CONFLICT: i32 = 5;

/// An error carrying a semantic process exit code.
#[derive(Debug)]
pub struct CodedError {
    pub code: i32,
    pub msg: String,
}

impl fmt::Display for CodedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for CodedError {}

/// A "not found" error (exit 3).
pub fn not_found(msg: impl Into<String>) -> CodedError {
    CodedError {
        code: EXIT_NOT_FOUND,
        msg: msg.into(),
    }
}

/// A "conflict — already exists" error (exit 5).
pub fn conflict(msg: impl Into<String>) -> CodedError {
    CodedError {
        code: EXIT_CONFLICT,
        msg: msg.into(),
    }
}

/// Returns the semantic exit code if the error wraps a [`CodedError`] or a
/// `bloxide_codegen::edit::EditError`, matching on it (e.g. for
/// `--if-not-exists` conflict tolerance).
pub fn code_of(err: &anyhow::Error) -> Option<i32> {
    if let Some(ce) = err.downcast_ref::<CodedError>() {
        return Some(ce.code);
    }
    match err.downcast_ref::<bloxide_codegen::edit::EditError>() {
        Some(bloxide_codegen::edit::EditError::Conflict(_)) => Some(EXIT_CONFLICT),
        Some(bloxide_codegen::edit::EditError::NotFound(_)) => Some(EXIT_NOT_FOUND),
        None => None,
    }
}

/// Print the error and exit with its semantic code (1 for uncoded errors).
pub fn exit_process(err: anyhow::Error) -> ! {
    let code = code_of(&err).unwrap_or(1);
    eprintln!("Error: {err:#}");
    std::process::exit(code);
}
