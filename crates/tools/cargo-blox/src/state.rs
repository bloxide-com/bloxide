// Copyright 2025 Bloxide, all rights reserved
//! Add or remove states from a blox's blox.toml.

use anyhow::bail;
use toml::Table;

use crate::toml_helpers::{
    blox_toml_path_for_blox, load_toml, save_toml, states_array_mut, topology_table_mut,
};

pub fn add_state(
    blox_name: &str,
    state_name: &str,
    parent: Option<&str>,
    composite: bool,
    terminal: bool,
    error: bool,
) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;

    let topology = topology_table_mut(&mut doc)?;
    let states = states_array_mut(topology)?;

    if states.iter().any(|s| {
        s.as_table()
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            == Some(state_name)
    }) {
        bail!("state '{}' already exists in {}", state_name, blox_name);
    }

    let mut state_table = toml::Value::Table(Table::new());
    let t = state_table.as_table_mut().unwrap();
    t.insert("name".into(), toml::Value::String(state_name.into()));
    if composite {
        t.insert("composite".into(), toml::Value::Boolean(true));
    }
    if let Some(p) = parent {
        t.insert("parent".into(), toml::Value::String(p.into()));
    }
    if terminal {
        t.insert("terminal".into(), toml::Value::Boolean(true));
    }
    if error {
        t.insert("error".into(), toml::Value::Boolean(true));
    }
    states.push(state_table);

    save_toml(&path, &doc)?;
    println!("Added state '{}' to {}", state_name, blox_name);
    Ok(())
}

pub fn remove_state(blox_name: &str, state_name: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;

    let topology = topology_table_mut(&mut doc)?;
    let states = states_array_mut(topology)?;

    let exists = states.iter().any(|s| {
        s.as_table()
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            == Some(state_name)
    });
    if !exists {
        bail!("state '{}' not found in {}", state_name, blox_name);
    }

    let children: Vec<String> = states
        .iter()
        .filter_map(|s| {
            let t = s.as_table()?;
            let p = t.get("parent")?.as_str()?;
            if p == state_name {
                t.get("name")?.as_str().map(String::from)
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

    states.retain(|s| {
        s.as_table()
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            != Some(state_name)
    });

    save_toml(&path, &doc)?;
    println!("Removed state '{}' from {}", state_name, blox_name);
    Ok(())
}
