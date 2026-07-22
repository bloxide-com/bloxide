// Copyright 2025 Bloxide, all rights reserved
//! `list-states` command — list the states defined in a blox's topology.

use anyhow::{bail, Context};
use serde::Serialize;

use crate::toml_helpers::{blox_toml_path_for_blox, load_toml, states_array, topology_table};

/// A single state row, used for both table and JSON output.
#[derive(Serialize)]
struct StateRow {
    name: String,
    initial: bool,
    composite: bool,
    terminal: bool,
    error: bool,
    parent: Option<String>,
}

/// Extract a state row from a `[[topology.states]]` table.
fn state_row_from_value(state: &toml::Value) -> Option<StateRow> {
    let table = state.as_table()?;
    let name = table.get("name")?.as_str()?.to_string();
    let initial = table
        .get("initial")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let composite = table
        .get("composite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let terminal = table
        .get("terminal")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let error = table
        .get("error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let parent = table
        .get("parent")
        .and_then(|v| v.as_str())
        .map(String::from);
    Some(StateRow {
        name,
        initial,
        composite,
        terminal,
        error,
        parent,
    })
}

/// List the states in a blox's topology.
///
/// Reads `crates/bloxes/<blox_name>/blox.toml` and prints the states in
/// either a padded table (default) or a pretty-printed JSON array
/// (`--json`).
pub fn list_states(blox_name: &str, json: bool) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    if !path.exists() {
        bail!(
            "blox.toml not found for blox '{}' at {}",
            blox_name,
            path.display()
        );
    }

    let doc = load_toml(&path).with_context(|| format!("failed to load {}", path.display()))?;

    let rows: Vec<StateRow> = topology_table(&doc)
        .and_then(|topo| states_array(topo))
        .map(|arr| {
            arr.iter()
                .filter_map(state_row_from_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if json {
        let out =
            serde_json::to_string_pretty(&rows).context("failed to serialize states to JSON")?;
        println!("{out}");
        return Ok(());
    }

    // Table output with fixed-width columns.
    println!(
        "{:<20} {:<8} {:<10} {:<10} {:<8} {:<20}",
        "NAME", "INITIAL", "COMPOSITE", "TERMINAL", "ERROR", "PARENT"
    );
    for row in &rows {
        println!(
            "{:<20} {:<8} {:<10} {:<10} {:<8} {:<20}",
            row.name,
            row.initial,
            row.composite,
            row.terminal,
            row.error,
            row.parent.as_deref().unwrap_or("")
        );
    }

    Ok(())
}
