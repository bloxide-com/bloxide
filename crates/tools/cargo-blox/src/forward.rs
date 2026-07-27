// Copyright 2025 Bloxide, all rights reserved
//! Forward commands to `cargo`.

use clap_cargo::Features;
use std::process::Command;

pub fn forward_to_cargo(cmd: &str, extra_args: &[String]) -> anyhow::Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let mut command = Command::new(&cargo);
    command.arg(cmd);

    for arg in extra_args {
        command.arg(arg);
    }

    let status = command.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

/// Generate code from blox.toml, then forward to a cargo subcommand.
///
/// Shared implementation for `build`, `check`, `test`, and `run` — they all
/// regenerate first, then forward to cargo with the same feature-flag
/// handling.
pub fn generate_then_forward(cmd: &str, cargo: Features, args: Vec<String>) -> anyhow::Result<()> {
    crate::generate::generate(None)?;
    let mut extra = Vec::new();
    if !cargo.features.is_empty() {
        extra.push("--features".to_string());
        extra.push(cargo.features.join(","));
    }
    if cargo.no_default_features {
        extra.push("--no-default-features".to_string());
    }
    if cargo.all_features {
        extra.push("--all-features".to_string());
    }
    extra.extend(args);
    forward_to_cargo(cmd, &extra)
}
