// Copyright 2025 Bloxide, all rights reserved
//! Add/remove entry and exit actions on a blox's blox.toml states (#112).
//!
//! Edit primitives live in `bloxide_codegen::edit` (the shared write path,
//! issue #96); this module is the CLI wrapper.

use crate::toml_helpers::{blox_toml_path_for_blox, load_toml, save_toml};

fn add_hook(
    hook: &str,
    blox_name: &str,
    state: &str,
    actions: Vec<String>,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    match bloxide_codegen::edit::add_hook(&mut doc, hook, state, actions, feature) {
        Ok(()) => {}
        Err(e) if if_not_exists && e.to_string().contains("already exists") => return Ok(()),
        Err(e) => anyhow::bail!("{} in {}", e, blox_name),
    }
    save_toml(&path, &doc)?;
    println!("Added {} hook for state {} to {}", hook, state, blox_name);
    Ok(())
}

fn remove_hook(hook: &str, blox_name: &str, state: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    bloxide_codegen::edit::remove_hook(&mut doc, hook, state)
        .map_err(|e| anyhow::anyhow!("{} in {}", e, blox_name))?;
    save_toml(&path, &doc)?;
    println!(
        "Removed {} hook for state {} from {}",
        hook, state, blox_name
    );
    Ok(())
}

pub fn add_entry(
    blox_name: &str,
    state: &str,
    actions: Vec<String>,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    add_hook("entry", blox_name, state, actions, feature, if_not_exists)
}

pub fn remove_entry(blox_name: &str, state: &str) -> anyhow::Result<()> {
    remove_hook("entry", blox_name, state)
}

pub fn add_exit(
    blox_name: &str,
    state: &str,
    actions: Vec<String>,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    add_hook("exit", blox_name, state, actions, feature, if_not_exists)
}

pub fn remove_exit(blox_name: &str, state: &str) -> anyhow::Result<()> {
    remove_hook("exit", blox_name, state)
}
