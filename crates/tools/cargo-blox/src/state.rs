// Copyright 2025 Bloxide, all rights reserved
//! Add or remove states from a blox's blox.toml.
//!
//! Edit primitives live in `bloxide_codegen::edit` (the shared write path,
//! issue #96); this module is the CLI wrapper.

use crate::toml_helpers::{blox_toml_path_for_blox, load_toml, save_toml};

pub fn add_state(
    blox_name: &str,
    state_name: &str,
    parent: Option<&str>,
    composite: bool,
    error: bool,
) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    bloxide_codegen::edit::add_state(&mut doc, state_name, parent, composite, error)
        .map_err(|e| anyhow::anyhow!("{} in {}", e, blox_name))?;
    save_toml(&path, &doc)?;
    println!("Added state '{}' to {}", state_name, blox_name);
    Ok(())
}

pub fn remove_state(blox_name: &str, state_name: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    bloxide_codegen::edit::remove_state(&mut doc, state_name)
        .map_err(|e| anyhow::anyhow!("{} in {}", e, blox_name))?;
    save_toml(&path, &doc)?;
    println!("Removed state '{}' from {}", state_name, blox_name);
    Ok(())
}
