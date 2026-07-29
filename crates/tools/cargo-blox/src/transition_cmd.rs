// Copyright 2025 Bloxide, all rights reserved
//! Add transitions to a blox's blox.toml.

use anyhow::bail;
use toml_edit::{ArrayOfTables, Table};

use crate::toml_helpers::{
    blox_toml_path_for_blox, load_toml, save_toml, topology_table_mut, transitions_array_mut,
};

fn state_event_of(t: &Table) -> (Option<&str>, Option<&str>) {
    (
        t.get("state").and_then(|v| v.as_str()),
        t.get("event").and_then(|v| v.as_str()),
    )
}

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

    let topology = topology_table_mut(&mut doc)?;
    let transitions = transitions_array_mut(topology)?;

    // Check for duplicate: state + event pair (exact string comparison).
    let duplicate = transitions
        .iter()
        .any(|t| state_event_of(t) == (Some(state), Some(event)));

    if duplicate {
        if if_not_exists {
            return Ok(());
        }
        bail!(
            "transition {} + {} already exists in {}",
            state,
            event,
            blox_name
        );
    }

    // Build the new transition table.
    let mut t = Table::new();
    t["state"] = toml_edit::value(state);
    t["event"] = toml_edit::value(event);
    t["target"] = toml_edit::value(target);

    if !actions.is_empty() {
        let mut arr = toml_edit::Array::new();
        for a in &actions {
            arr.push(a.as_str());
        }
        t["actions"] = toml_edit::value(arr);
    }

    if !guards.is_empty() {
        let mut guards_arr = ArrayOfTables::new();
        for guard_str in &guards {
            // Split on the LAST ':' to separate condition from target.
            // This handles '::' in Rust paths within the condition.
            let (condition, guard_target) = match guard_str.rsplit_once(':') {
                Some((cond, tgt)) => (cond, tgt),
                None => {
                    bail!(
                        "invalid guard '{}' — expected 'condition:target' (missing ':')",
                        guard_str
                    );
                }
            };
            let mut guard_table = Table::new();
            guard_table["condition"] = toml_edit::value(condition);
            guard_table["target"] = toml_edit::value(guard_target);
            guards_arr.push(guard_table);
        }
        t["guards"] = toml_edit::Item::ArrayOfTables(guards_arr);
    }

    if let Some(feat) = feature {
        t["feature"] = toml_edit::value(feat);
    }

    transitions.push(t);

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

    let topology = topology_table_mut(&mut doc)?;
    let transitions = transitions_array_mut(topology)?;

    let exists = transitions
        .iter()
        .any(|t| state_event_of(t) == (Some(state), Some(event)));
    if !exists {
        bail!(
            "transition {} + {} not found in {}",
            state,
            event,
            blox_name
        );
    }

    let indices: Vec<usize> = transitions
        .iter()
        .enumerate()
        .filter_map(|(i, e)| (state_event_of(&e) == (Some(state), Some(event))).then_some(i))
        .collect();
    for i in indices.into_iter().rev() {
        transitions.remove(i);
    }

    save_toml(&path, &doc)?;
    println!(
        "Removed transition {} + {} from {}",
        state, event, blox_name
    );
    Ok(())
}
