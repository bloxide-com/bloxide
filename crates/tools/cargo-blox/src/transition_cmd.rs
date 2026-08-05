// Copyright 2025 Bloxide, all rights reserved
//! Add transitions to a blox's blox.toml.
//!
//! Edit primitives live in `bloxide_codegen::edit` (the shared write path,
//! issue #96); this module is the CLI wrapper.

use crate::toml_helpers::{blox_toml_path_for_blox, load_toml, save_toml};
use anyhow::Context;

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
        Err(e) if if_not_exists && crate::exit::code_of(&e) == Some(crate::exit::EXIT_CONFLICT) => {
            return Ok(())
        }
        Err(e) => return Err(e).with_context(|| format!("in {}", blox_name)),
    }
    save_toml(&path, &doc)?;
    println!(
        "Added transition {} + {} -> {} to {}",
        state, event, target, blox_name
    );
    Ok(())
}

pub fn remove_transition(
    blox_name: &str,
    state: &str,
    event: &str,
    feature: Option<&str>,
) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    bloxide_codegen::edit::remove_transition(&mut doc, state, event, feature)
        .with_context(|| format!("in {}", blox_name))?;
    save_toml(&path, &doc)?;
    match feature {
        Some(feat) => println!(
            "Removed transition {} + {} (feature {}) from {}",
            state, event, feat, blox_name
        ),
        None => println!(
            "Removed transition {} + {} from {}",
            state, event, blox_name
        ),
    }
    Ok(())
}
