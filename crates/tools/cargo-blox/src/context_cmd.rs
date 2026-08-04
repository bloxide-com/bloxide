// Copyright 2025 Bloxide, all rights reserved
//! Manage blox.toml context declarations (#121): `[[context.uses]]`,
//! `[[context.fields]]`, and `[[context.actions]]` entries.

use anyhow::bail;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

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

/// All context field names contributed by a `[[context.uses]]` entry: the
/// top-level `field` key (single-field shape) plus every sub-field `name`
/// (multi-field shape — inline `fields = [...]` or nested
/// `[[context.uses.fields]]`; multi-field entries have no top-level `field`).
fn use_entry_field_names(u: &Table) -> Vec<&str> {
    let mut names = Vec::new();
    if let Some(f) = u.get("field").and_then(|v| v.as_str()) {
        names.push(f);
    }
    match u.get("fields") {
        Some(Item::ArrayOfTables(sub)) => {
            for t in sub.iter() {
                if let Some(n) = t.get("name").and_then(|v| v.as_str()) {
                    names.push(n);
                }
            }
        }
        Some(Item::Value(Value::Array(arr))) => {
            for v in arr.iter() {
                if let Some(n) = v
                    .as_inline_table()
                    .and_then(|t| t.get("name"))
                    .and_then(|n| n.as_str())
                {
                    names.push(n);
                }
            }
        }
        _ => {}
    }
    names
}

/// A validated field of a new `[[context.uses]]` entry.
struct UseField {
    name: String,
    ty: String,
    role: String,
}

/// Parses a `--sub-field name:ty:role` spec. The type may contain `::` paths
/// (e.g. `ActorRef<bloxide_timer::TimerCommand, R>`), so the name splits at
/// the first `:` and the role at the last.
fn parse_sub_field(spec: &str) -> anyhow::Result<UseField> {
    let (name, rest) = spec
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("malformed --sub-field '{spec}' — expected name:ty:role"))?;
    let (ty, role) = rest
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("malformed --sub-field '{spec}' — expected name:ty:role"))?;
    if name.is_empty() || ty.is_empty() {
        bail!("malformed --sub-field '{spec}' — name and ty must be non-empty");
    }
    Ok(UseField {
        name: name.to_string(),
        ty: ty.to_string(),
        role: role.to_string(),
    })
}

pub fn add_use(
    blox_name: &str,
    field: Option<&str>,
    field_type: Option<&str>,
    role: Option<&str>,
    sub_fields: &[String],
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    const ROLES: [&str; 2] = ["ctor", "state"];

    // Two mutually exclusive shapes: single-field (--field/--field-type/
    // --role, given together) or multi-field (--sub-field name:ty:role,
    // repeatable).
    let single = match (field, field_type, role) {
        (Some(f), Some(t), Some(r)) => Some(UseField {
            name: f.to_string(),
            ty: t.to_string(),
            role: r.to_string(),
        }),
        (None, None, None) => None,
        _ => bail!("--field, --field-type, and --role must be given together"),
    };
    if single.is_some() && !sub_fields.is_empty() {
        bail!("--field/--field-type/--role cannot be combined with --sub-field");
    }
    if single.is_none() && sub_fields.is_empty() {
        bail!("nothing to add — pass --field/--field-type/--role or at least one --sub-field");
    }

    let single_shape = single.is_some();
    let mut new_fields: Vec<UseField> = single.into_iter().collect();
    for spec in sub_fields {
        new_fields.push(parse_sub_field(spec)?);
    }
    let mut seen = std::collections::BTreeSet::new();
    for f in &new_fields {
        if !ROLES.contains(&f.role.as_str()) {
            bail!(
                "unknown role '{}' — expected one of: {}",
                f.role,
                ROLES.join(", ")
            );
        }
        if !seen.insert(&f.name) {
            bail!("duplicate field name '{}' in the new entry", f.name);
        }
    }

    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    let context = context_table_mut(&mut doc)?;
    let uses = uses_array_mut(context)?;

    // Dedup key: any field name the entry would contribute. Multi-field
    // entries carry no top-level `field`, so sub-field names must be checked
    // too — a collision would duplicate a context struct field.
    let existing: Vec<&str> = uses.iter().flat_map(use_entry_field_names).collect();
    for f in &new_fields {
        if existing.contains(&f.name.as_str()) {
            if if_not_exists {
                return Ok(());
            }
            return Err(crate::exit::conflict(format!(
                "context use field '{}' already exists in {}",
                f.name, blox_name
            ))
            .into());
        }
    }

    let mut t = Table::new();
    if single_shape {
        let f = &new_fields[0];
        t["field"] = toml_edit::value(f.name.as_str());
        t["field_type"] = toml_edit::value(f.ty.as_str());
        t["role"] = toml_edit::value(f.role.as_str());
        if let Some(feat) = feature {
            t["feature"] = toml_edit::value(feat);
        }
    } else {
        if let Some(feat) = feature {
            t["feature"] = toml_edit::value(feat);
        }
        // Inline-table array, matching the pool/blox.toml style.
        let mut arr = Array::new();
        for f in &new_fields {
            let mut it = InlineTable::new();
            it.insert("name", Value::from(f.name.as_str()));
            it.insert("ty", Value::from(f.ty.as_str()));
            it.insert("role", Value::from(f.role.as_str()));
            let mut v = Value::InlineTable(it);
            v.decor_mut().set_prefix("\n    ");
            arr.push_formatted(v);
        }
        arr.set_trailing_comma(true);
        arr.set_trailing("\n");
        t["fields"] = toml_edit::value(arr);
    }
    uses.push(t);

    save_toml(&path, &doc)?;
    if single_shape {
        let f = &new_fields[0];
        println!(
            "Added context use '{}' ({}) to {}",
            f.name, f.role, blox_name
        );
    } else {
        let names = new_fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("Added context use fields [{}] to {}", names, blox_name);
    }
    Ok(())
}

/// Removes sub-fields named `field` from a nested `[[context.uses.fields]]`
/// array. Returns true if anything was removed.
fn remove_sub_field_from_tables(sub: &mut ArrayOfTables, field: &str) -> bool {
    let indices: Vec<usize> = sub
        .iter()
        .enumerate()
        .filter_map(|(i, f)| (f.get("name").and_then(|v| v.as_str()) == Some(field)).then_some(i))
        .collect();
    let removed = !indices.is_empty();
    for i in indices.into_iter().rev() {
        sub.remove(i);
    }
    removed
}

/// Removes sub-fields named `field` from an inline `fields = [...]` array.
/// Returns true if anything was removed.
fn remove_sub_field_from_array(arr: &mut Array, field: &str) -> bool {
    let indices: Vec<usize> = arr
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            (v.as_inline_table()
                .and_then(|t| t.get("name"))
                .and_then(|n| n.as_str())
                == Some(field))
            .then_some(i)
        })
        .collect();
    let removed = !indices.is_empty();
    for i in indices.into_iter().rev() {
        arr.remove(i);
    }
    removed
}

/// True when the entry's `fields` item exists but holds no sub-fields.
fn fields_item_empty(u: &Table) -> bool {
    match u.get("fields") {
        Some(Item::ArrayOfTables(sub)) => sub.is_empty(),
        Some(Item::Value(Value::Array(arr))) => arr.is_empty(),
        _ => false,
    }
}

pub fn remove_use(blox_name: &str, field: &str) -> anyhow::Result<()> {
    let path = blox_toml_path_for_blox(blox_name);
    let mut doc = load_toml(&path)?;
    let context = context_table_mut(&mut doc)?;
    let uses = uses_array_mut(context)?;

    let mut removed = false;

    // Single-field shape: the top-level `field` key identifies the entry —
    // remove the whole entry.
    let indices: Vec<usize> = uses
        .iter()
        .enumerate()
        .filter_map(|(i, u)| (u.get("field").and_then(|v| v.as_str()) == Some(field)).then_some(i))
        .collect();
    for i in indices.into_iter().rev() {
        uses.remove(i);
        removed = true;
    }

    // Multi-field shape: no top-level `field` — entries are identified by the
    // sub-field names they contribute. Remove just the matching sub-field
    // (both the inline `fields = [...]` and nested `[[context.uses.fields]]`
    // forms) and drop an entry that ends up with no sub-fields.
    let mut emptied: Vec<usize> = Vec::new();
    for (i, u) in uses.iter_mut().enumerate() {
        let sub_removed = match u.get_mut("fields") {
            Some(Item::ArrayOfTables(sub)) => remove_sub_field_from_tables(sub, field),
            Some(Item::Value(Value::Array(arr))) => remove_sub_field_from_array(arr, field),
            _ => false,
        };
        if sub_removed {
            removed = true;
            if fields_item_empty(u) {
                emptied.push(i);
            }
        }
    }
    for i in emptied.into_iter().rev() {
        uses.remove(i);
    }

    if !removed {
        return Err(crate::exit::not_found(format!(
            "context use field '{}' not found in {}",
            field, blox_name
        ))
        .into());
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
    // multi-field [[context.uses]] sub-fields (inline `fields = [...]` or
    // nested `[[context.uses.fields]]`) — whichever matches the name.
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

    // Multi-field shape, as in remove_use: remove just the matching sub-field
    // (both the inline `fields = [...]` and nested `[[context.uses.fields]]`
    // forms) and drop an entry that ends up with no sub-fields.
    let mut emptied: Vec<usize> = Vec::new();
    for (i, u) in uses.iter_mut().enumerate() {
        let sub_removed = match u.get_mut("fields") {
            Some(Item::ArrayOfTables(sub)) => remove_sub_field_from_tables(sub, name),
            Some(Item::Value(Value::Array(arr))) => remove_sub_field_from_array(arr, name),
            _ => false,
        };
        if sub_removed {
            removed = true;
            if fields_item_empty(u) {
                emptied.push(i);
            }
        }
    }
    for i in emptied.into_iter().rev() {
        uses.remove(i);
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
    returns: Option<&str>,
    feature: Option<&str>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    // Mirror of the codegen/lint hard rule: "ActionResult" is the only
    // recognized returns value.
    if let Some(r) = returns {
        if r != "ActionResult" {
            bail!(
                "action \"{name}\" has returns = \"{r}\" — the only recognized value is \"ActionResult\""
            );
        }
    }

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
    if let Some(r) = returns {
        t["returns"] = toml_edit::value(r);
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
