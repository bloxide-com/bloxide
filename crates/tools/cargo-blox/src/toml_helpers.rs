// Copyright 2025 Bloxide, all rights reserved
//! Shared helpers for loading, mutating, and saving blox.toml files.
//!
//! These helpers centralize the TOML access patterns used across the
//! `cargo-blox` subcommands (state, message, list, …) so that each
//! command can focus on its domain logic instead of repeating the same
//! load/mutate/save boilerplate.
//!
//! All mutation goes through `toml_edit::DocumentMut`, which preserves the
//! original formatting and comments (including the copyright header) —
//! reserializing with `toml::to_string` strips both.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use toml_edit::{ArrayOfTables, DocumentMut, Table};

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

/// Loads and parses a TOML file into a `toml_edit::DocumentMut`.
pub(crate) fn load_toml(path: &Path) -> anyhow::Result<DocumentMut> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    content
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", path.display()))
}

/// Saves a TOML document back to disk, preserving formatting and comments.
pub(crate) fn save_toml(path: &Path, doc: &DocumentMut) -> anyhow::Result<()> {
    fs::write(path, doc.to_string()).with_context(|| format!("failed to write {}", path.display()))
}

/// Read-only accessor for the `[topology]` table.
pub(crate) fn topology_table(root: &DocumentMut) -> Option<&Table> {
    root.get("topology")?.as_table()
}

/// Mutable accessor for the `[topology]` table, creating it if missing.
pub(crate) fn topology_table_mut(root: &mut DocumentMut) -> anyhow::Result<&mut Table> {
    if root.get("topology").is_none() {
        root["topology"] = toml_edit::Item::Table(Table::new());
    }
    root.get_mut("topology")
        .and_then(|t| t.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!("topology is not a table"))
}

/// Read-only accessor for the `[[topology.states]]` array.
pub(crate) fn states_array(topology: &Table) -> Option<&ArrayOfTables> {
    topology.get("states")?.as_array_of_tables()
}

/// Mutable accessor for the `[[topology.states]]` array, creating it if missing.
pub(crate) fn states_array_mut(topology: &mut Table) -> anyhow::Result<&mut ArrayOfTables> {
    if topology.get("states").is_none() {
        topology["states"] = toml_edit::Item::ArrayOfTables(ArrayOfTables::new());
    }
    topology
        .get_mut("states")
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("topology.states is not an array of tables"))
}

/// Read-only accessor for the `[[topology.transitions]]` array.
pub(crate) fn transitions_array(topology: &Table) -> Option<&ArrayOfTables> {
    topology.get("transitions")?.as_array_of_tables()
}

/// Mutable accessor for the `[[topology.transitions]]` array, creating it if missing.
pub(crate) fn transitions_array_mut(topology: &mut Table) -> anyhow::Result<&mut ArrayOfTables> {
    if topology.get("transitions").is_none() {
        topology["transitions"] = toml_edit::Item::ArrayOfTables(ArrayOfTables::new());
    }
    topology
        .get_mut("transitions")
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("topology.transitions is not an array of tables"))
}
