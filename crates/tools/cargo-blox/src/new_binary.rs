// Copyright 2025 Bloxide, all rights reserved
//! Scaffold a new example (wiring) crate: `examples/<name>/system.toml`.
//!
//! The example's crate (Cargo.toml, build.rs, src/main.rs, src/lib.rs,
//! src/generated/) is NOT written here — it is materialized by
//! `cargo blox generate` from the system.toml manifest into
//! `target/bloxide-generated/examples/<name>/` (invariant #18: system.toml is
//! the single source of truth for wiring). The source directory is not a
//! workspace member, so the root Cargo.toml is left untouched.

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::utils::{to_camel_case, validate_runtime, workspace_root_or_cwd};

pub fn new_binary(name: &str, runtime: &str) -> Result<()> {
    let root = workspace_root_or_cwd()?;
    new_binary_in(&root, name, runtime)
}

pub(crate) fn new_binary_in(root: &Path, name: &str, runtime: &str) -> Result<()> {
    validate_runtime(runtime)?;

    let name_snake = name.to_lowercase().replace("-", "_");
    let name_camel = to_camel_case(name);

    let examples_dir = root.join("examples");
    let crate_dir = examples_dir.join(&name_snake);
    fs::create_dir_all(&crate_dir)?;

    let system_toml = format!(
        r#"# Copyright 2025 Bloxide, all rights reserved
# Wiring manifest for the {name_camel} system — the single source of truth
# for the materialized example crate (regenerate: cargo blox generate).
[system]
runtime = "{runtime}"
name = "{name_snake}"

[[actors]]
name = "{name_snake}"
blox = "{name_snake}-blox"

  # Bootstrap: one Tick so the demo actor runs and exits cleanly.
  [[actors.bootstrap]]
  message = "{name_camel}Msg::Tick"

[[supervision]]
supervisor = "bloxide-supervisor"
strategy = "when_any_done"
children = ["{name_snake}"]

  [supervision.policies]
  {name_snake} = {{ stop = true }}
"#
    );
    let system_path = crate_dir.join("system.toml");
    fs::write(&system_path, system_toml)?;

    println!("Created: {}", system_path.display());
    Ok(())
}
