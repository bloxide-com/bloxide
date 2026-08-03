// Copyright 2025 Bloxide, all rights reserved
//! `cargo blox viz` — launch the visualizer from the CLI (#113).
//!
//! - `cargo blox viz` — launch the Dioxus fullstack visualizer server,
//!   scanning the current workspace
//! - `cargo blox viz --export <dir>` — export blox specs as JSON (no server)
//! - `--port <n>` — server port (default 8080)
//! - `--open` — open the browser after launch

use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context};

use crate::utils::find_workspace_root_from;

pub fn viz(export: Option<PathBuf>, port: u16, open_browser: bool) -> anyhow::Result<()> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string())
    }));
    let root = find_workspace_root_from(&manifest_dir).unwrap_or(manifest_dir);

    // ── Export mode: write JSON specs and exit (no server) ──────────────
    if let Some(output_dir) = export {
        let specs = bloxide_viz_export::export_workspace(&root)
            .map_err(|e| anyhow::anyhow!("export failed: {}", e))?;
        bloxide_viz_export::write_specs_to_json(&specs, &output_dir)
            .map_err(|e| anyhow::anyhow!("failed to write JSON: {}", e))?;
        println!(
            "bloxide: exported {} spec(s) to {}",
            specs.len(),
            output_dir.display()
        );
        return Ok(());
    }

    // ── Server mode: launch the visualizer fullstack server ─────────────
    let viz_dir = root.join("tools/bloxide-visualizer");
    if !viz_dir.join("Dioxus.toml").exists() {
        bail!(crate::exit::not_found(format!(
            "visualizer not found at {} — `cargo blox viz` must run from a bloxide checkout",
            viz_dir.display()
        )));
    }

    // The dioxus CLI is required: a fullstack app's WASM client bundle is
    // built by dx, not by plain `cargo run`.
    let dx_available = Command::new("dx")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !dx_available {
        bail!("the dioxus CLI is required for the visualizer — install it with: cargo install dioxus-cli");
    }

    println!(
        "bloxide: launching visualizer at http://localhost:{} (workspace: {})",
        port,
        root.display()
    );

    let status = Command::new("dx")
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--open")
        .arg(if open_browser { "true" } else { "false" })
        .arg("--hot-reload")
        .arg("false")
        // The visualizer reads this env var as its data source (#113).
        .env("BLOXIDE_VIZ_WORKSPACE", &root)
        .current_dir(&viz_dir)
        .status()
        .context("failed to launch dx serve")?;
    if !status.success() {
        bail!("visualizer exited with {}", status);
    }
    Ok(())
}
