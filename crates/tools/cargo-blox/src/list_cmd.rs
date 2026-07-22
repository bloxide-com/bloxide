// Copyright 2025 Bloxide, all rights reserved
//! `list-states` command — list the states defined in a blox's topology.

use anyhow::{bail, Context};
use serde::Serialize;

use crate::toml_helpers::{
    blox_toml_path_for_blox, blox_toml_path_for_messages, load_toml, states_array, topology_table,
    transitions_array,
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

/// A single field within a message variant, used for JSON output.
#[derive(Serialize)]
struct MessageFieldRow {
    name: String,
    ty: String,
}

/// A single message variant row, used for both table and JSON output.
#[derive(Serialize)]
struct MessageVariantRow {
    name: String,
    fields: Vec<MessageFieldRow>,
}

/// Extract a field row from a `[[messages.variants.fields]]` table.
fn message_field_row_from_value(field: &toml::Value) -> Option<MessageFieldRow> {
    let table = field.as_table()?;
    let name = table.get("name")?.as_str()?.to_string();
    let ty = table.get("ty")?.as_str()?.to_string();
    Some(MessageFieldRow { name, ty })
}

/// Extract a variant row from a `[[messages.variants]]` table.
fn message_variant_row_from_value(variant: &toml::Value) -> Option<MessageVariantRow> {
    let table = variant.as_table()?;
    let name = table.get("name")?.as_str()?.to_string();
    let fields = table
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(message_field_row_from_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(MessageVariantRow { name, fields })
}

/// List the message variants in a messages crate.
///
/// Reads `crates/messages/<crate_name>/blox.toml` and prints the message
/// variants in either a padded table (default) or a pretty-printed JSON
/// array (`--json`).
pub fn list_messages(crate_name: &str, json: bool) -> anyhow::Result<()> {
    let path = blox_toml_path_for_messages(crate_name);
    if !path.exists() {
        bail!(
            "blox.toml not found for messages crate '{}' at {}",
            crate_name,
            path.display()
        );
    }

    let doc = load_toml(&path).with_context(|| format!("failed to load {}", path.display()))?;

    // Collect variants from all [[messages]] entries.
    let messages_array = doc.as_table().and_then(|t| t.get("messages")?.as_array());
    let rows: Vec<MessageVariantRow> = messages_array
        .map(|arr| {
            arr.iter()
                .filter_map(|msg| {
                    msg.as_table()
                        .and_then(|t| t.get("variants")?.as_array())
                        .map(|variants| {
                            variants
                                .iter()
                                .filter_map(message_variant_row_from_value)
                                .collect::<Vec<_>>()
                        })
                })
                .flatten()
                .collect()
        })
        .unwrap_or_default();

    if json {
        let out = serde_json::to_string_pretty(&rows)
            .context("failed to serialize message variants to JSON")?;
        println!("{out}");
        return Ok(());
    }

    // Table output with fixed-width columns.
    println!("{:<20} FIELDS", "VARIANT");
    for row in &rows {
        let fields = if row.fields.is_empty() {
            "(none)".to_string()
        } else {
            row.fields
                .iter()
                .map(|f| format!("{}: {}", f.name, f.ty))
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!("{:<20} {}", row.name, fields);
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


// `list-bloxes` command — list all blox crates in the workspace with summary counts.

/// A single summary row for a blox, used for both table and JSON output.
#[derive(Serialize)]
struct BloxSummaryRow {
    name: String,
    states: usize,
    transitions: usize,
    messages: usize,
}

/// Count the total number of message variants across all `[[messages]]` entries.
fn count_message_variants(doc: &toml::Value) -> usize {
    doc.as_table()
        .and_then(|t| t.get("messages")?.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|msg| {
                    msg.as_table()
                        .and_then(|t| t.get("variants")?.as_array())
                        .map(|variants| variants.len())
                })
                .sum()
        })
        .unwrap_or(0)
}

/// List all blox crates in the workspace.
///
/// Scans `crates/bloxes/*/blox.toml` and prints a summary table (NAME,
/// STATES, TRANSITIONS, MESSAGES) or a pretty-printed JSON array
/// (`--json`).  Results are sorted alphabetically by blox name.
pub fn list_bloxes(json: bool) -> anyhow::Result<()> {
    let bloxes_dir = std::path::Path::new("crates/bloxes");
    let mut rows: Vec<BloxSummaryRow> = Vec::new();

    if bloxes_dir.exists() {
        let entries = std::fs::read_dir(bloxes_dir)
            .with_context(|| format!("failed to read {}", bloxes_dir.display()))?;
        for entry in entries {
            let entry = entry.context("failed to read directory entry")?;
            let path = entry.path();
            // Only consider subdirectories that contain a blox.toml.
            if !path.is_dir() {
                continue;
            }
            let toml_path = path.join("blox.toml");
            if !toml_path.exists() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let doc = load_toml(&toml_path)
                .with_context(|| format!("failed to load {}", toml_path.display()))?;
            let states = topology_table(&doc)
                .and_then(states_array)
                .map(|arr| arr.len())
                .unwrap_or(0);
            let transitions = topology_table(&doc)
                .and_then(transitions_array)
                .map(|arr| arr.len())
                .unwrap_or(0);
            let messages = count_message_variants(&doc);
            rows.push(BloxSummaryRow {
                name,
                states,
                transitions,
                messages,
            });
        }
    }

    rows.sort_by(|a, b| a.name.cmp(&b.name));

    if json {
        let out = serde_json::to_string_pretty(&rows)
            .context("failed to serialize blox summaries to JSON")?;
        println!("{}", out);
        return Ok(());
    }

    // Table output with fixed-width columns.
    println!(
        "{:<20} {:<8} {:<12} {:<10}",
        "NAME", "STATES", "TRANSITIONS", "MESSAGES"
    );
    for row in &rows {
        println!(
            "{:<20} {:<8} {:<12} {:<10}",
            row.name, row.states, row.transitions, row.messages
        );
    }

    Ok(())
}
