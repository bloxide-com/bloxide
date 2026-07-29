// Copyright 2025 Bloxide, all rights reserved
//! Shared blox.toml edit primitives (issue #96): the single write path used
//! by `cargo-blox` CLI commands and the visualizer's save functions.
//!
//! All mutations go through `toml_edit::DocumentMut`, preserving the original
//! formatting and comments. Each function takes the parsed document and
//! performs one logical edit; IO helpers ([`load_blox_toml`] /
//! [`save_blox_toml`]) cover the read/write.

use std::path::Path;

use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table};

/// Load a blox.toml (or system.toml) into an editable document.
pub fn load_blox_toml(path: &Path) -> anyhow::Result<DocumentMut> {
    let content = std::fs::read_to_string(path)?;
    Ok(content.parse::<DocumentMut>()?)
}

/// Save an edited document back to disk, preserving formatting/comments.
pub fn save_blox_toml(path: &Path, doc: &DocumentMut) -> anyhow::Result<()> {
    std::fs::write(path, doc.to_string())?;
    Ok(())
}

// ── Internal table accessors ────────────────────────────────────────────────

fn topology_table_mut(doc: &mut DocumentMut) -> anyhow::Result<&mut Table> {
    if doc.get("topology").is_none() {
        doc["topology"] = Item::Table(Table::new());
    }
    doc.get_mut("topology")
        .and_then(|t| t.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!("topology is not a table"))
}

fn array_mut<'a>(topology: &'a mut Table, key: &str) -> anyhow::Result<&'a mut ArrayOfTables> {
    if topology.get(key).is_none() {
        topology[key] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    topology
        .get_mut(key)
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("topology.{} is not an array of tables", key))
}

fn remove_where(arr: &mut ArrayOfTables, matches: impl Fn(&Table) -> bool) {
    let indices: Vec<usize> = arr
        .iter()
        .enumerate()
        .filter_map(|(i, t)| matches(t).then_some(i))
        .collect();
    for i in indices.into_iter().rev() {
        arr.remove(i);
    }
}

fn string_array(items: &[String]) -> Item {
    let mut arr = Array::new();
    for i in items {
        arr.push(i.as_str());
    }
    toml_edit::value(arr)
}

// ── States ──────────────────────────────────────────────────────────────────

/// Add a `[[topology.states]]` entry. Fails if a state with the same name exists.
pub fn add_state(
    doc: &mut DocumentMut,
    name: &str,
    parent: Option<&str>,
    composite: bool,
    error: bool,
) -> anyhow::Result<()> {
    let topology = topology_table_mut(doc)?;
    let states = array_mut(topology, "states")?;

    if states
        .iter()
        .any(|s| s.get("name").and_then(|v| v.as_str()) == Some(name))
    {
        anyhow::bail!("state '{}' already exists", name);
    }

    let mut t = Table::new();
    t["name"] = toml_edit::value(name);
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
    Ok(())
}

/// Remove a state. Fails if it doesn't exist or other states reference it
/// as their parent.
pub fn remove_state(doc: &mut DocumentMut, name: &str) -> anyhow::Result<()> {
    let topology = topology_table_mut(doc)?;
    let states = array_mut(topology, "states")?;

    if !states
        .iter()
        .any(|s| s.get("name").and_then(|v| v.as_str()) == Some(name))
    {
        anyhow::bail!("state '{}' not found", name);
    }

    let children: Vec<String> = states
        .iter()
        .filter_map(|s| {
            let p = s.get("parent")?.as_str()?;
            if p == name {
                s.get("name")?.as_str().map(String::from)
            } else {
                None
            }
        })
        .collect();
    if !children.is_empty() {
        anyhow::bail!(
            "cannot remove state '{}': states [{}] reference it as parent",
            name,
            children.join(", ")
        );
    }

    remove_where(states, |s| {
        s.get("name").and_then(|v| v.as_str()) == Some(name)
    });
    Ok(())
}

// ── Transitions ─────────────────────────────────────────────────────────────

/// Add a `[[topology.transitions]]` entry. Guards are `"condition:target"`
/// strings (split on the LAST `:` to allow `::` in conditions).
#[allow(clippy::too_many_arguments)]
pub fn add_transition(
    doc: &mut DocumentMut,
    state: &str,
    event: &str,
    target: &str,
    actions: Vec<String>,
    guards: Vec<String>,
    feature: Option<&str>,
) -> anyhow::Result<()> {
    let topology = topology_table_mut(doc)?;
    let transitions = array_mut(topology, "transitions")?;

    let duplicate = transitions.iter().any(|t| {
        t.get("state").and_then(|v| v.as_str()) == Some(state)
            && t.get("event").and_then(|v| v.as_str()) == Some(event)
    });
    if duplicate {
        anyhow::bail!("transition {} + {} already exists", state, event);
    }

    let mut t = Table::new();
    t["state"] = toml_edit::value(state);
    t["event"] = toml_edit::value(event);
    t["target"] = toml_edit::value(target);

    if !actions.is_empty() {
        t["actions"] = string_array(&actions);
    }

    if !guards.is_empty() {
        let mut guards_arr = ArrayOfTables::new();
        for guard_str in &guards {
            let (condition, guard_target) = match guard_str.rsplit_once(':') {
                Some((cond, tgt)) => (cond, tgt),
                None => {
                    anyhow::bail!(
                        "invalid guard '{}' — expected 'condition:target' (missing ':')",
                        guard_str
                    );
                }
            };
            let mut g = Table::new();
            g["condition"] = toml_edit::value(condition);
            g["target"] = toml_edit::value(guard_target);
            guards_arr.push(g);
        }
        t["guards"] = Item::ArrayOfTables(guards_arr);
    }

    if let Some(feat) = feature {
        t["feature"] = toml_edit::value(feat);
    }

    transitions.push(t);
    Ok(())
}

/// Remove a transition by its natural key (state + event pattern).
pub fn remove_transition(doc: &mut DocumentMut, state: &str, event: &str) -> anyhow::Result<()> {
    let topology = topology_table_mut(doc)?;
    let transitions = array_mut(topology, "transitions")?;

    let exists = transitions.iter().any(|t| {
        t.get("state").and_then(|v| v.as_str()) == Some(state)
            && t.get("event").and_then(|v| v.as_str()) == Some(event)
    });
    if !exists {
        anyhow::bail!("transition {} + {} not found", state, event);
    }

    remove_where(transitions, |t| {
        t.get("state").and_then(|v| v.as_str()) == Some(state)
            && t.get("event").and_then(|v| v.as_str()) == Some(event)
    });
    Ok(())
}

// ── Entry / exit hooks ──────────────────────────────────────────────────────

/// Add a `[[topology.entry]]` or `[[topology.exit]]` hook (one per state).
pub fn add_hook(
    doc: &mut DocumentMut,
    hook: &str,
    state: &str,
    actions: Vec<String>,
    feature: Option<&str>,
) -> anyhow::Result<()> {
    if hook != "entry" && hook != "exit" {
        anyhow::bail!("hook must be \"entry\" or \"exit\", got '{}'", hook);
    }
    let topology = topology_table_mut(doc)?;
    let entries = array_mut(topology, hook)?;

    if entries
        .iter()
        .any(|e| e.get("state").and_then(|v| v.as_str()) == Some(state))
    {
        anyhow::bail!("{} hook for state {} already exists", hook, state);
    }

    let mut t = Table::new();
    t["state"] = toml_edit::value(state);
    if !actions.is_empty() {
        t["actions"] = string_array(&actions);
    }
    if let Some(f) = feature {
        t["feature"] = toml_edit::value(f);
    }
    entries.push(t);
    Ok(())
}

/// Remove an entry or exit hook for a state.
pub fn remove_hook(doc: &mut DocumentMut, hook: &str, state: &str) -> anyhow::Result<()> {
    let topology = topology_table_mut(doc)?;
    let entries = array_mut(topology, hook)?;

    if !entries
        .iter()
        .any(|e| e.get("state").and_then(|v| v.as_str()) == Some(state))
    {
        anyhow::bail!("{} hook for state {} not found", hook, state);
    }

    remove_where(entries, |e| {
        e.get("state").and_then(|v| v.as_str()) == Some(state)
    });
    Ok(())
}
