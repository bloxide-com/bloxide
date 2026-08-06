// Copyright 2025 Bloxide, all rights reserved
//! `cargo blox wire` — materialize the example crate for a system.toml
//! wiring manifest into the generated workspace.

use anyhow::Result;
use std::path::PathBuf;

use crate::utils::find_workspace_root;

pub fn wire(system: Option<PathBuf>, output: Option<PathBuf>, run: bool) -> Result<()> {
    let workspace_root = find_workspace_root()?;
    let system_path = system.unwrap_or_else(|| workspace_root.join("system.toml"));

    if !system_path.exists() {
        anyhow::bail!(crate::exit::not_found(format!(
            "system.toml not found at {}",
            system_path.display()
        )));
    }

    // Explicit --output: write just the generated main.rs to that path.
    if let Some(output_path) = output {
        let main_rs =
            bloxide_codegen::generate_system_wiring_from_toml(&system_path, &workspace_root)?;
        std::fs::create_dir_all(output_path.parent().unwrap())?;
        std::fs::write(&output_path, &main_rs)?;
        println!("bloxide: generated {}", output_path.display());
    }

    // Materialize the full example crate into the generated workspace.
    let crate_name = bloxide_codegen::example_crate::sync_example(&system_path, &workspace_root)?;
    let generated_manifest = workspace_root
        .join(bloxide_codegen::blox_crate::GENERATED_WORKSPACE_DIR)
        .join("Cargo.toml");
    println!(
        "bloxide: materialized example crate '{}' in {}",
        crate_name,
        generated_manifest
            .parent()
            .unwrap()
            .join("examples")
            .join(&crate_name)
            .display()
    );

    if run {
        println!("bloxide: running example '{}'...", crate_name);
        let status = std::process::Command::new("cargo")
            .arg("run")
            .arg("--manifest-path")
            .arg(&generated_manifest)
            .arg("-p")
            .arg(&crate_name)
            .status()
            .map_err(|e| anyhow::anyhow!("failed to spawn cargo run: {e}"))?;

        if !status.success() {
            anyhow::bail!("cargo run -p {} exited with status {}", crate_name, status);
        }
    }

    Ok(())
}
