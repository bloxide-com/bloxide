// Copyright 2025 Bloxide, all rights reserved
//! Subcommands for adding and removing message variants in blox.toml files.
//!
//! All mutation goes through `toml_edit::DocumentMut`, which preserves the
//! original formatting and comments (including the copyright header) —
//! reserializing with `toml::to_string` strips both (see toml_helpers.rs).

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use crate::toml_helpers::{blox_toml_path_for_messages, load_toml, save_toml};

pub fn add_message(
    crate_name: &str,
    variant_name: &str,
    fields: Vec<(String, String)>,
) -> anyhow::Result<()> {
    let toml_path = blox_toml_path_for_messages(crate_name);
    let mut doc = load_toml(&toml_path)?;

    let msg_name = ensure_messages_table(&mut doc, crate_name)?;
    let msg_table = find_messages_table(&mut doc, &msg_name)?;

    if has_variant(msg_table, variant_name) {
        return Err(crate::exit::conflict(format!(
            "variant '{}' already exists in messages",
            variant_name
        ))
        .into());
    }

    let mut variant = Table::new();
    variant["name"] = toml_edit::value(variant_name);
    if !fields.is_empty() {
        let mut fields_arr = ArrayOfTables::new();
        for (fname, fty) in fields {
            let mut field = Table::new();
            field["name"] = toml_edit::value(fname);
            field["ty"] = toml_edit::value(fty);
            fields_arr.push(field);
        }
        variant["fields"] = Item::ArrayOfTables(fields_arr);
    }

    let variants = msg_table
        .entry("variants")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or_else(|| anyhow::anyhow!("messages.variants is not an array of tables"))?;
    variants.push(variant);

    save_toml(&toml_path, &doc)?;
    println!(
        "Added variant '{}' to {}",
        variant_name,
        toml_path.display()
    );
    Ok(())
}

pub fn remove_message(crate_name: &str, variant_name: &str) -> anyhow::Result<()> {
    let toml_path = blox_toml_path_for_messages(crate_name);
    let mut doc = load_toml(&toml_path)?;

    let msg_array = doc
        .get_mut("messages")
        .and_then(|m| m.as_array_of_tables_mut())
        .ok_or_else(|| {
            anyhow::Error::from(crate::exit::not_found(
                "no [[messages]] table found".to_string(),
            ))
        })?;

    let mut found = false;
    for msg_entry in msg_array.iter_mut() {
        // Find the index of the variant (if any) in this table's variants.
        let idx = msg_entry
            .get("variants")
            .and_then(|v| v.as_array_of_tables())
            .and_then(|variants| {
                variants
                    .iter()
                    .position(|v| v.get("name").and_then(|n| n.as_str()) == Some(variant_name))
            });
        if let Some(idx) = idx {
            if let Some(variants) = msg_entry
                .get_mut("variants")
                .and_then(|v| v.as_array_of_tables_mut())
            {
                variants.remove(idx);
                found = true;
            }
        }
    }

    if !found {
        return Err(crate::exit::not_found(format!(
            "variant '{}' not found in any messages table",
            variant_name
        ))
        .into());
    }

    save_toml(&toml_path, &doc)?;
    println!(
        "Removed variant '{}' from {}",
        variant_name,
        toml_path.display()
    );
    Ok(())
}

/// Conventional message enum name for a `*-messages` crate (e.g.
/// `ping-pong-messages` → `PingPongMsg`).
fn crate_name_to_msg_name(crate_name: &str) -> String {
    let base = crate_name.strip_suffix("-messages").unwrap_or(crate_name);
    base.split(['-', '_'])
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    upper + chars.as_str()
                }
                None => String::new(),
            }
        })
        .collect::<String>()
        + "Msg"
}

/// Ensure the crate has a `[[messages]]` array with at least one named table.
/// Returns the enum name to add variants to: the conventional
/// `XxxMsg` table if it exists, otherwise the first table, otherwise a
/// newly created conventional one.
fn ensure_messages_table(doc: &mut DocumentMut, crate_name: &str) -> anyhow::Result<String> {
    let msg_name = crate_name_to_msg_name(crate_name);
    let root = doc.as_table_mut();

    if root.get("messages").is_none() {
        let mut table = Table::new();
        table["name"] = toml_edit::value(msg_name.clone());
        table["visibility"] = toml_edit::value("pub");
        let mut arr = ArrayOfTables::new();
        arr.push(table);
        root["messages"] = Item::ArrayOfTables(arr);
        return Ok(msg_name);
    }

    let tables = root
        .get_mut("messages")
        .and_then(|m| m.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("[[messages]] is not an array of tables"))?;
    if tables.is_empty() {
        let mut table = Table::new();
        table["name"] = toml_edit::value(msg_name.clone());
        table["visibility"] = toml_edit::value("pub");
        tables.push(table);
        return Ok(msg_name);
    }
    // Prefer the conventional XxxMsg table; otherwise target the first.
    let has_conventional = tables
        .iter()
        .any(|t| t.get("name").and_then(|n| n.as_str()) == Some(msg_name.as_str()));
    if has_conventional {
        Ok(msg_name)
    } else {
        Ok(tables
            .iter()
            .next()
            .and_then(|t| t.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or(&msg_name)
            .to_string())
    }
}

/// Get (by name) the `[[messages]]` table to mutate.
fn find_messages_table<'a>(
    doc: &'a mut DocumentMut,
    msg_name: &str,
) -> anyhow::Result<&'a mut Table> {
    let tables = doc
        .get_mut("messages")
        .and_then(|m| m.as_array_of_tables_mut())
        .ok_or_else(|| {
            anyhow::Error::from(crate::exit::not_found(
                "no [[messages]] table found".to_string(),
            ))
        })?;
    tables
        .iter_mut()
        .find(|t| t.get("name").and_then(|n| n.as_str()) == Some(msg_name))
        .ok_or_else(|| {
            anyhow::Error::from(crate::exit::not_found(format!(
                "no [[messages]] table named '{}'",
                msg_name
            )))
        })
}

fn has_variant(msg_table: &Table, name: &str) -> bool {
    msg_table
        .get("variants")
        .and_then(|v| v.as_array_of_tables())
        .map(|arr| {
            arr.iter()
                .any(|v| v.get("name").and_then(|n| n.as_str()) == Some(name))
        })
        .unwrap_or(false)
}
