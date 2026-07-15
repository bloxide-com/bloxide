// Copyright 2025 Bloxide, all rights reserved
//! Forward commands to `cargo`.

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
