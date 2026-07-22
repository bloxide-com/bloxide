// Copyright 2025 Bloxide, all rights reserved
//! Add transitions to a blox's blox.toml.

use anyhow::bail;
use toml::Table;

use crate::toml_helpers::{
    blox_toml_path_for_blox, load_toml, save_toml, topology_table_mut, transitions_array_mut,
};

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
    let duplicate = transitions.iter().any(|t| {
        let table = match t.as_table() {
            Some(t) => t,
            None => return false,
        };
        let existing_state = table.get("state").and_then(|v| v.as_str()) == Some(state);
        let existing_event = table.get("event").and_then(|v| v.as_str()) == Some(event);
        existing_state && existing_event
    });

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
    let mut transition_table = toml::Value::Table(Table::new());
    let t = transition_table.as_table_mut().unwrap();
    t.insert("state".into(), toml::Value::String(state.to_string()));
    t.insert("event".into(), toml::Value::String(event.to_string()));
    t.insert("target".into(), toml::Value::String(target.to_string()));

    if !actions.is_empty() {
        let actions_arr: Vec<toml::Value> = actions
            .iter()
            .map(|a| toml::Value::String(a.clone()))
            .collect();
        t.insert("actions".into(), toml::Value::Array(actions_arr));
    }

    if !guards.is_empty() {
        let mut guards_arr: Vec<toml::Value> = Vec::with_capacity(guards.len());
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
            guard_table.insert(
                "condition".into(),
                toml::Value::String(condition.to_string()),
            );
            guard_table.insert(
                "target".into(),
                toml::Value::String(guard_target.to_string()),
            );
            guards_arr.push(toml::Value::Table(guard_table));
        }
        t.insert("guards".into(), toml::Value::Array(guards_arr));
    }

    if let Some(feat) = feature {
        t.insert("feature".into(), toml::Value::String(feat.to_string()));
    }

    transitions.push(transition_table);

    save_toml(&path, &doc)?;
    println!(
        "Added transition {} + {} -> {} to {}",
        state, event, target, blox_name
    );
    Ok(())
}
