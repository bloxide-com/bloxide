// Copyright 2025 Bloxide, all rights reserved
//! `list-states` command — list the states defined in a blox's topology.

use anyhow::{bail, Context};
use serde::Serialize;

use crate::toml_helpers::{
    blox_toml_path_for_blox, load_toml, states_array, topology_table, transitions_array,
};

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

// `list-transitions` command — list the transitions defined in a blox's topology.

/// A single guard row within a transition, used for JSON output.
#[derive(Serialize)]
struct GuardRow {
    condition: String,
    target: String,
}

/// A single transition row, used for both table and JSON output.
#[derive(Serialize)]
struct TransitionRow {
    state: String,
    event: String,
    target: String,
    actions: Vec<String>,
    guards: Vec<GuardRow>,
    feature: Option<String>,
}

/// Extract a guard row from a `[[topology.transitions.guards]]` table.
fn guard_row_from_value(guard: &toml::Value) -> Option<GuardRow> {
    let table = guard.as_table()?;
    let condition = table.get("condition")?.as_str()?.to_string();
    let target = table.get("target")?.as_str()?.to_string();
    Some(GuardRow { condition, target })
}

/// Extract a transition row from a `[[topology.transitions]]` table.
fn transition_row_from_value(transition: &toml::Value) -> Option<TransitionRow> {
    let table = transition.as_table()?;
    let state = table.get("state")?.as_str()?.to_string();
    let event = table.get("event")?.as_str()?.to_string();
    let target = table.get("target")?.as_str()?.to_string();
    let actions = table
        .get("actions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let guards = table
        .get("guards")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(guard_row_from_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let feature = table
        .get("feature")
        .and_then(|v| v.as_str())
        .map(String::from);
    Some(TransitionRow {
        state,
        event,
        target,
        actions,
        guards,
        feature,
    })
}

/// List the transitions in a blox's topology.
///
/// Reads `crates/bloxes/<blox_name>/blox.toml` and prints the transitions in
/// either a padded table (default) or a pretty-printed JSON array
/// (`--json`).
pub fn list_transitions(blox_name: &str, json: bool) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    if !path.exists() {
        bail!(
            "blox.toml not found for blox '{}' at {}",
            blox_name,
            path.display()
        );
    }

    let doc = load_toml(&path).with_context(|| format!("failed to load {}", path.display()))?;

    let rows: Vec<TransitionRow> = topology_table(&doc)
        .and_then(|topo| transitions_array(topo))
        .map(|arr| {
            arr.iter()
                .filter_map(transition_row_from_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if json {
        let out = serde_json::to_string_pretty(&rows)
            .context("failed to serialize transitions to JSON")?;
        println!("{out}");
        return Ok(());
    }

    // Table output with fixed-width columns.
    println!(
        "{:<20} {:<30} {:<20} {:<30} {:<8} {:<20}",
        "STATE", "EVENT", "TARGET", "ACTIONS", "GUARDS", "FEATURE"
    );
    for row in &rows {
        let actions = if row.actions.is_empty() {
            "—".to_string()
        } else {
            row.actions.join(", ")
        };
        let feature = row.feature.as_deref().unwrap_or("—");
        println!(
            "{:<20} {:<30} {:<20} {:<30} {:<8} {:<20}",
            row.state,
            row.event,
            row.target,
            actions,
            row.guards.len(),
            feature
        );
    }

    Ok(())
}
