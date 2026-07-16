// Copyright 2025 Bloxide, all rights reserved
//! `cargo blox wire` — generate a binary main.rs from a system.toml manifest.

use std::path::PathBuf;
use anyhow::Result;

pub fn wire(system: Option<PathBuf>, output: Option<PathBuf>, run: bool) -> Result<()> {
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

    if run {
        let system_dir = system_path.parent().unwrap();
        let cargo_toml_path = system_dir.join("Cargo.toml");
        let cargo_toml_content = std::fs::read_to_string(&cargo_toml_path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", cargo_toml_path.display(), e))?;
        let crate_name = parse_package_name(&cargo_toml_content)
            .ok_or_else(|| anyhow::anyhow!("could not find [package] name in {}", cargo_toml_path.display()))?;

        println!("bloxide: running crate '{}'...", crate_name);
        let status = std::process::Command::new("cargo")
            .arg("run")
            .arg("-p")
            .arg(&crate_name)
            .current_dir(&workspace_root)
            .status()
            .map_err(|e| anyhow::anyhow!("failed to spawn cargo run: {e}"))?;

        if !status.success() {
            anyhow::bail!("cargo run -p {} exited with status {}", crate_name, status);
        }
    }

    Ok(())
}

/// Extract the `name = "..."` value from the `[package]` section of a Cargo.toml.
fn parse_package_name(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package {
            if let Some(name) = trimmed.strip_prefix("name") {
                let name = name.trim_start();
                if let Some(rest) = name.strip_prefix('=') {
                    let value = rest.trim();
                    let value = value.trim_matches('"');
                    return Some(value.to_string());
                }
            }
        }
    }
    None
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
