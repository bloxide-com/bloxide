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

use crate::utils::find_workspace_root;

/// Anchor for all relative layout paths: the workspace root when inside a
/// Cargo workspace, otherwise the current directory. Commands run from a
/// subdirectory still find the right files.
fn layout_root() -> PathBuf {
    find_workspace_root().unwrap_or_else(|_| PathBuf::from("."))
}

/// Returns the blox.toml path for a blox: `<workspace>/bloxes/<blox_name>/blox.toml`
/// (pure-TOML layout).
pub(crate) fn blox_toml_path_for_blox(blox_name: &str) -> PathBuf {
    layout_root()
        .join("bloxes")
        .join(blox_name)
        .join("blox.toml")
}

/// Returns `<workspace>/crates/messages/<crate_name>/blox.toml`.
pub(crate) fn blox_toml_path_for_messages(crate_name: &str) -> PathBuf {
    layout_root()
        .join("crates/messages")
        .join(crate_name)
        .join("blox.toml")
}

/// Returns `<workspace>/examples/<app_name>/system.toml`.
pub(crate) fn system_toml_path_for_app(app_name: &str) -> PathBuf {
    layout_root()
        .join("examples")
        .join(app_name)
        .join("system.toml")
}

/// Returns the workspace root used for recursive discovery (lint, …).
pub(crate) fn discovery_root() -> PathBuf {
    layout_root()
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

/// Read-only accessor for the `[[topology.states]]` array.
pub(crate) fn states_array(topology: &Table) -> Option<&ArrayOfTables> {
    topology.get("states")?.as_array_of_tables()
}

/// Read-only accessor for the `[[topology.transitions]]` array.
pub(crate) fn transitions_array(topology: &Table) -> Option<&ArrayOfTables> {
    topology.get("transitions")?.as_array_of_tables()
}
