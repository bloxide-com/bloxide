// Copyright 2025 Bloxide, all rights reserved
//! `cargo blox wire` — generate a binary main.rs from a system.toml manifest.

use std::path::PathBuf;
use anyhow::Result;

pub fn wire(system: Option<PathBuf>, output: Option<PathBuf>) -> Result<()> {
    let workspace_root = find_workspace_root()?;
    let system_path = system.unwrap_or_else(|| workspace_root.join("system.toml"));

    if !system_path.exists() {
        anyhow::bail!("system.toml not found at {}", system_path.display());
    }

    let main_rs = bloxide_codegen::generate_system_wiring_from_toml(
        &system_path,
        &workspace_root,
    )?;

    let output_path = output.unwrap_or_else(|| {
        // Default: src/main.rs in the same directory as system.toml
        system_path.parent().unwrap().join("src").join("main.rs")
    });

    std::fs::create_dir_all(output_path.parent().unwrap())?;
    std::fs::write(&output_path, &main_rs)?;
    println!("bloxide: generated {}", output_path.display());
    Ok(())
}

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
