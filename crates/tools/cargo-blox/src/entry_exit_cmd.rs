// Copyright 2025 Bloxide, all rights reserved
//! Add/remove entry and exit actions on a blox's blox.toml states (#112).

use anyhow::bail;
use toml_edit::{ArrayOfTables, Table};

use crate::toml_helpers::{blox_toml_path_for_blox, load_toml, save_toml, topology_table_mut};

/// Which hook array to edit: `[[topology.entry]]` or `[[topology.exit]]`.
#[derive(Clone, Copy)]
enum Hook {
    Entry,
    Exit,
}

impl Hook {
    fn key(self) -> &'static str {
        match self {
            Hook::Entry => "entry",
            Hook::Exit => "exit",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Hook::Entry => "entry",
            Hook::Exit => "exit",
        }
    }
}

fn hook_array_mut(topology: &mut Table, hook: Hook) -> anyhow::Result<&mut ArrayOfTables> {
    if topology.get(hook.key()).is_none() {
        topology[hook.key()] = toml_edit::Item::ArrayOfTables(ArrayOfTables::new());
    }
    topology
        .get_mut(hook.key())
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("topology.{} is not an array of tables", hook.key()))
}

fn state_of(entry: &Table) -> Option<&str> {
    entry.get("state").and_then(|v| v.as_str())
}

fn add_hook(
    hook: Hook,
    blox_name: &str,
    state: &str,
    actions: Vec<String>,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;

    let topology = topology_table_mut(&mut doc)?;
    let entries = hook_array_mut(topology, hook)?;

    // Duplicate check: one hook entry per state.
    let duplicate = entries.iter().any(|e| state_of(e) == Some(state));
    if duplicate {
        if if_not_exists {
            return Ok(());
        }
        bail!(
            "{} hook for state {} already exists in {}",
            hook.label(),
            state,
            blox_name
        );
    }

    let mut entry_table = Table::new();
    entry_table["state"] = toml_edit::value(state);
    if !actions.is_empty() {
        let mut arr = toml_edit::Array::new();
        for a in &actions {
            arr.push(a.as_str());
        }
        entry_table["actions"] = toml_edit::value(arr);
    }
    if let Some(feat) = feature {
        entry_table["feature"] = toml_edit::value(feat);
    }

    entries.push(entry_table);

    save_toml(&path, &doc)?;
    println!(
        "Added {} hook for state {} to {}",
        hook.label(),
        state,
        blox_name
    );
    Ok(())
}

fn remove_hook(hook: Hook, blox_name: &str, state: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;

    let topology = topology_table_mut(&mut doc)?;
    let entries = hook_array_mut(topology, hook)?;

    let exists = entries.iter().any(|e| state_of(e) == Some(state));
    if !exists {
        bail!(
            "{} hook for state {} not found in {}",
            hook.label(),
            state,
            blox_name
        );
    }

    let indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| (state_of(&e) == Some(state)).then_some(i))
        .collect();
    for i in indices.into_iter().rev() {
        entries.remove(i);
    }

    save_toml(&path, &doc)?;
    println!(
        "Removed {} hook for state {} from {}",
        hook.label(),
        state,
        blox_name
    );
    Ok(())
}

pub fn add_entry(
    blox_name: &str,
    state: &str,
    actions: Vec<String>,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    add_hook(
        Hook::Entry,
        blox_name,
        state,
        actions,
        feature,
        if_not_exists,
    )
}

pub fn remove_entry(blox_name: &str, state: &str) -> anyhow::Result<()> {
    remove_hook(Hook::Entry, blox_name, state)
}

pub fn add_exit(
    blox_name: &str,
    state: &str,
    actions: Vec<String>,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    add_hook(
        Hook::Exit,
        blox_name,
        state,
        actions,
        feature,
        if_not_exists,
    )
}

pub fn remove_exit(blox_name: &str, state: &str) -> anyhow::Result<()> {
    remove_hook(Hook::Exit, blox_name, state)
}
