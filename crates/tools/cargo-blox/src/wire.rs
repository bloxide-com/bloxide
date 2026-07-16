// Copyright 2025 Bloxide, all rights reserved
//! `cargo blox wire` — generate a binary main.rs from a system.toml manifest.

use anyhow::Result;
use std::path::PathBuf;

pub fn wire(system: Option<PathBuf>, output: Option<PathBuf>) -> Result<()> {
    // NOTE: system_wiring codegen is disabled until Stage 2 (issue #83) is
    // completed. The `generate_system_wiring_from_toml` function was removed
    // from bloxide-codegen because it didn't compile.
    let _ = (system, output);
    anyhow::bail!(
        "`cargo blox wire` is not yet available — system_wiring codegen is \
         under development (see GitHub issue #83)"
    )
}

#[allow(dead_code)]
fn find_workspace_root() -> Result<PathBuf> {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let mut current = manifest_dir.as_path();
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return Ok(current.to_path_buf());
                }
            }
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => anyhow::bail!("workspace root not found"),
        }
    }
}
