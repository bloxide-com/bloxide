// Copyright 2025 Bloxide, all rights reserved
//! Add transitions to a blox's blox.toml.
//!
//! Edit primitives live in `bloxide_codegen::edit` (the shared write path,
//! issue #96); this module is the CLI wrapper.

use crate::toml_helpers::{blox_toml_path_for_blox, load_toml, save_toml};

#[allow(clippy::too_many_arguments)]
pub fn add_transition(
    blox_name: &str,
    state: &str,
    event: &str,
    target: &str,
    actions: Vec<String>,
    guards: Vec<String>,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    match bloxide_codegen::edit::add_transition(
        &mut doc, state, event, target, actions, guards, feature,
    ) {
        Ok(()) => {}
        Err(e) if if_not_exists && e.to_string().contains("already exists") => return Ok(()),
        Err(e) => anyhow::bail!("{} in {}", e, blox_name),
    }
    save_toml(&path, &doc)?;
    println!(
        "Added transition {} + {} -> {} to {}",
        state, event, target, blox_name
    );
    Ok(())
}

pub fn remove_transition(blox_name: &str, state: &str, event: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    bloxide_codegen::edit::remove_transition(&mut doc, state, event)
        .map_err(|e| anyhow::anyhow!("{} in {}", e, blox_name))?;
    save_toml(&path, &doc)?;
    println!(
        "Removed transition {} + {} from {}",
        state, event, blox_name
    );
    Ok(())
}
