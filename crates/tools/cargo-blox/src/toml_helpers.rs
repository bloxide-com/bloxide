// Copyright 2025 Bloxide, all rights reserved
//! Shared helpers for loading, mutating, and saving blox.toml files.
//!
//! These helpers centralize the TOML access patterns used across the
//! `cargo-blox` subcommands (state, message, list, …) so that each
//! command can focus on its domain logic instead of repeating the same
//! load/mutate/save boilerplate.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use toml::Table;

/// Returns the path `crates/bloxes/<blox_name>/blox.toml` relative to the
/// current working directory.
pub(crate) fn blox_toml_path_for_blox(blox_name: &str) -> PathBuf {
    Path::new("crates/bloxes").join(blox_name).join("blox.toml")
}

/// Returns the path `crates/messages/<crate_name>/blox.toml` relative to the
/// current working directory.
pub(crate) fn blox_toml_path_for_messages(crate_name: &str) -> PathBuf {
    Path::new("crates/messages")
        .join(crate_name)
        .join("blox.toml")
}

/// Loads and parses a TOML file into a [`toml::Value`].
pub(crate) fn load_toml(path: &Path) -> anyhow::Result<toml::Value> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(value)
}

/// Serializes a [`toml::Value`] back to a TOML file, pretty-printed.
pub(crate) fn save_toml(path: &Path, value: &toml::Value) -> anyhow::Result<()> {
    let content = toml::to_string_pretty(value)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Read-only accessor for the `[topology]` table of a blox.toml document.
///
/// Returns `None` if the root is not a table or there is no `topology` key.
pub(crate) fn topology_table(root: &toml::Value) -> Option<&Table> {
    root.as_table()?.get("topology")?.as_table()
}

/// Mutable accessor for the `[topology]` table, creating it if missing.
pub(crate) fn topology_table_mut(root: &mut toml::Value) -> anyhow::Result<&mut Table> {
    let table = root
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("blox.toml root is not a table"))?;
    if !table.contains_key("topology") {
        table.insert("topology".into(), toml::Value::Table(Table::new()));
    }
    table
        .get_mut("topology")
        .and_then(|v| v.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!("topology is not a table"))
}

/// Read-only accessor for the `[[topology.states]]` array.
pub(crate) fn states_array(topology: &Table) -> Option<&Vec<toml::Value>> {
    topology.get("states")?.as_array()
}

/// Mutable accessor for the `[[topology.states]]` array, creating it if missing.
pub(crate) fn states_array_mut(topology: &mut Table) -> anyhow::Result<&mut Vec<toml::Value>> {
    if !topology.contains_key("states") {
        topology.insert("states".into(), toml::Value::Array(Vec::new()));
    }
    topology
        .get_mut("states")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| anyhow::anyhow!("topology.states is not an array"))
}

/// Read-only accessor for the `[[topology.transitions]]` array.
#[allow(dead_code)]
pub(crate) fn transitions_array(topology: &Table) -> Option<&Vec<toml::Value>> {
    topology.get("transitions")?.as_array()
}

/// Mutable accessor for the `[[topology.transitions]]` array, creating it if missing.
pub(crate) fn transitions_array_mut(topology: &mut Table) -> anyhow::Result<&mut Vec<toml::Value>> {
    if !topology.contains_key("transitions") {
        topology.insert("transitions".into(), toml::Value::Array(Vec::new()));
    }
    topology
        .get_mut("transitions")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| anyhow::anyhow!("topology.transitions is not an array"))
}
