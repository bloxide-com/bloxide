// Copyright 2025 Bloxide, all rights reserved
//! Add or remove states from a blox's blox.toml.

use anyhow::bail;
use toml_edit::Table;

use crate::toml_helpers::{
    blox_toml_path_for_blox, load_toml, save_toml, states_array_mut, topology_table_mut,
};

fn name_of(s: &Table) -> Option<&str> {
    s.get("name").and_then(|v| v.as_str())
}

pub fn add_state(
    blox_name: &str,
    state_name: &str,
    parent: Option<&str>,
    composite: bool,
    error: bool,
) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;

    let topology = topology_table_mut(&mut doc)?;
    let states = states_array_mut(topology)?;

    if states.iter().any(|s| name_of(s) == Some(state_name)) {
        bail!("state '{}' already exists in {}", state_name, blox_name);
    }

    let mut t = Table::new();
    t["name"] = toml_edit::value(state_name);
    if composite {
        t["composite"] = toml_edit::value(true);
    }
    if let Some(p) = parent {
        t["parent"] = toml_edit::value(p);
    }
    if error {
        t["error"] = toml_edit::value(true);
    }
    states.push(t);

    save_toml(&path, &doc)?;
    println!("Added state '{}' to {}", state_name, blox_name);
    Ok(())
}

pub fn remove_state(blox_name: &str, state_name: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;

    let topology = topology_table_mut(&mut doc)?;
    let states = states_array_mut(topology)?;

    let exists = states.iter().any(|s| name_of(s) == Some(state_name));
    if !exists {
        bail!("state '{}' not found in {}", state_name, blox_name);
    }

    let children: Vec<String> = states
        .iter()
        .filter_map(|s| {
            let p = s.get("parent")?.as_str()?;
            if p == state_name {
                name_of(s).map(String::from)
            } else {
                None
            }
        })
        .collect();
    if !children.is_empty() {
        bail!(
            "cannot remove state '{}': states [{}] reference it as parent",
            state_name,
            children.join(", ")
        );
    }

    let indices: Vec<usize> = states
        .iter()
        .enumerate()
        .filter_map(|(i, e)| (name_of(&e) == Some(state_name)).then_some(i))
        .collect();
    for i in indices.into_iter().rev() {
        states.remove(i);
    }

    save_toml(&path, &doc)?;
    println!("Removed state '{}' from {}", state_name, blox_name);
    Ok(())
}
