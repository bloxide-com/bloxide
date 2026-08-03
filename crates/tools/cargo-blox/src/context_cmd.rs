// Copyright 2025 Bloxide, all rights reserved
//! Manage blox.toml context declarations (#121): `[[context.uses]]`,
//! `[[context.fields]]`, and `[[context.actions]]` entries.

use anyhow::bail;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table};

use crate::toml_helpers::{blox_toml_path_for_blox, load_toml, save_toml};

fn context_table_mut(doc: &mut DocumentMut) -> anyhow::Result<&mut Table> {
    if doc.get("context").is_none() {
        doc["context"] = Item::Table(Table::new());
    }
    doc.get_mut("context")
        .and_then(|t| t.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!("[context] is not a table"))
}

fn uses_array_mut(context: &mut Table) -> anyhow::Result<&mut ArrayOfTables> {
    if context.get("uses").is_none() {
        context["uses"] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    context
        .get_mut("uses")
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("[[context.uses]] is not an array of tables"))
}

fn fields_array_mut(context: &mut Table) -> anyhow::Result<&mut ArrayOfTables> {
    if context.get("fields").is_none() {
        context["fields"] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    context
        .get_mut("fields")
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("[[context.fields]] is not an array of tables"))
}

fn actions_array_mut(context: &mut Table) -> anyhow::Result<&mut ArrayOfTables> {
    if context.get("actions").is_none() {
        context["actions"] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    context
        .get_mut("actions")
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("[[context.actions]] is not an array of tables"))
}

fn string_array(items: &[String]) -> Item {
    let mut arr = Array::new();
    for i in items {
        arr.push(i.as_str());
    }
    toml_edit::value(arr)
}

// ── add-use / remove-use ────────────────────────────────────────────────────

pub fn add_use(
    blox_name: &str,
    field: &str,
    field_type: &str,
    role: &str,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    const ROLES: [&str; 2] = ["ctor", "state"];
    if !ROLES.contains(&role) {
        bail!(
            "unknown role '{}' — expected one of: {}",
            role,
            ROLES.join(", ")
        );
    }

    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    let context = context_table_mut(&mut doc)?;
    let uses = uses_array_mut(context)?;

    let duplicate = uses
        .iter()
        .any(|u| u.get("field").and_then(|v| v.as_str()) == Some(field));
    if duplicate {
        if if_not_exists {
            return Ok(());
        }
        return Err(crate::exit::conflict(format!(
            "context use field '{}' already exists in {}",
            field, blox_name
        ))
        .into());
    }

    let mut t = Table::new();
    t["field"] = toml_edit::value(field);
    t["field_type"] = toml_edit::value(field_type);
    t["role"] = toml_edit::value(role);
    if let Some(f) = feature {
        t["feature"] = toml_edit::value(f);
    }
    uses.push(t);

    save_toml(&path, &doc)?;
    println!("Added context use '{}' ({}) to {}", field, role, blox_name);
    Ok(())
}

pub fn remove_use(blox_name: &str, field: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    let context = context_table_mut(&mut doc)?;
    let uses = uses_array_mut(context)?;

    let exists = uses
        .iter()
        .any(|u| u.get("field").and_then(|v| v.as_str()) == Some(field));
    if !exists {
        return Err(crate::exit::not_found(format!(
            "context use field '{}' not found in {}",
            field, blox_name
        ))
        .into());
    }

    let indices: Vec<usize> = uses
        .iter()
        .enumerate()
        .filter_map(|(i, u)| (u.get("field").and_then(|v| v.as_str()) == Some(field)).then_some(i))
        .collect();
    for i in indices.into_iter().rev() {
        uses.remove(i);
    }

    save_toml(&path, &doc)?;
    println!("Removed context use '{}' from {}", field, blox_name);
    Ok(())
}

// ── add-field / remove-field ────────────────────────────────────────────────

pub fn add_field(
    blox_name: &str,
    name: &str,
    ty: &str,
    default: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    let context = context_table_mut(&mut doc)?;
    let fields = fields_array_mut(context)?;

    let duplicate = fields
        .iter()
        .any(|f| f.get("name").and_then(|v| v.as_str()) == Some(name));
    if duplicate {
        if if_not_exists {
            return Ok(());
        }
        return Err(crate::exit::conflict(format!(
            "context field '{}' already exists in {}",
            name, blox_name
        ))
        .into());
    }

    let mut t = Table::new();
    t["name"] = toml_edit::value(name);
    t["type"] = toml_edit::value(ty);
    if let Some(d) = default {
        t["default"] = toml_edit::value(d);
    }
    fields.push(t);

    save_toml(&path, &doc)?;
    println!("Added context field '{}': {} to {}", name, ty, blox_name);
    Ok(())
}

pub fn remove_field(blox_name: &str, name: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    let context = context_table_mut(&mut doc)?;

    // Remove from [[context.fields]], single-field [[context.uses]], or
    // multi-field [[context.uses.fields]] — whichever matches the name.
    let mut removed = false;

    let fields = fields_array_mut(context)?;
    let indices: Vec<usize> = fields
        .iter()
        .enumerate()
        .filter_map(|(i, f)| (f.get("name").and_then(|v| v.as_str()) == Some(name)).then_some(i))
        .collect();
    for i in indices.into_iter().rev() {
        fields.remove(i);
        removed = true;
    }

    let uses = uses_array_mut(context)?;
    let use_indices: Vec<usize> = uses
        .iter()
        .enumerate()
        .filter_map(|(i, u)| (u.get("field").and_then(|v| v.as_str()) == Some(name)).then_some(i))
        .collect();
    for i in use_indices.into_iter().rev() {
        uses.remove(i);
        removed = true;
    }
    for u in uses.iter_mut() {
        if let Some(sub) = u.get_mut("fields").and_then(|f| f.as_array_of_tables_mut()) {
            let sub_indices: Vec<usize> = sub
                .iter()
                .enumerate()
                .filter_map(|(i, f)| {
                    (f.get("name").and_then(|v| v.as_str()) == Some(name)).then_some(i)
                })
                .collect();
            for i in sub_indices.into_iter().rev() {
                sub.remove(i);
                removed = true;
            }
        }
    }

    if !removed {
        return Err(crate::exit::not_found(format!(
            "context field '{}' not found in {}",
            name, blox_name
        ))
        .into());
    }

    save_toml(&path, &doc)?;
    println!("Removed context field '{}' from {}", name, blox_name);
    Ok(())
}

// ── add-action / remove-action ──────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn add_action(
    blox_name: &str,
    name: &str,
    fields: Vec<String>,
    crate_name: Option<&str>,
    module: Option<&str>,
    fn_name: Option<&str>,
    event_payload: Option<&str>,
    impl_required: bool,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    let context = context_table_mut(&mut doc)?;
    let actions = actions_array_mut(context)?;

    let duplicate = actions
        .iter()
        .any(|a| a.get("name").and_then(|v| v.as_str()) == Some(name));
    if duplicate {
        if if_not_exists {
            return Ok(());
        }
        return Err(crate::exit::conflict(format!(
            "context action '{}' already exists in {}",
            name, blox_name
        ))
        .into());
    }

    let mut t = Table::new();
    t["name"] = toml_edit::value(name);
    if let Some(f) = fn_name {
        t["fn_name"] = toml_edit::value(f);
    }
    if let Some(c) = crate_name {
        t["crate"] = toml_edit::value(c);
    }
    if let Some(m) = module {
        t["module"] = toml_edit::value(m);
    }
    if !fields.is_empty() {
        t["fields"] = string_array(&fields);
    }
    if let Some(p) = event_payload {
        t["event_payload"] = toml_edit::value(p);
    }
    if impl_required {
        t["impl_required"] = toml_edit::value(true);
    }
    if let Some(f) = feature {
        t["feature"] = toml_edit::value(f);
    }
    actions.push(t);

    save_toml(&path, &doc)?;
    println!("Added context action '{}' to {}", name, blox_name);
    Ok(())
}

pub fn remove_action(blox_name: &str, name: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    let context = context_table_mut(&mut doc)?;
    let actions = actions_array_mut(context)?;

    let exists = actions
        .iter()
        .any(|a| a.get("name").and_then(|v| v.as_str()) == Some(name));
    if !exists {
        return Err(crate::exit::not_found(format!(
            "context action '{}' not found in {}",
            name, blox_name
        ))
        .into());
    }

    let indices: Vec<usize> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| (a.get("name").and_then(|v| v.as_str()) == Some(name)).then_some(i))
        .collect();
    for i in indices.into_iter().rev() {
        actions.remove(i);
    }

    save_toml(&path, &doc)?;
    println!("Removed context action '{}' from {}", name, blox_name);
    Ok(())
}
