// Copyright 2025 Bloxide, all rights reserved
//! Subcommands for adding and removing message variants in blox.toml files.

use anyhow::{bail, Context};
use std::fs;
use std::path::{Path, PathBuf};

pub fn add_message(
    crate_name: &str,
    variant_name: &str,
    fields: Vec<(String, String)>,
) -> anyhow::Result<()> {
    let toml_path = blox_toml_path(crate_name);
    let content = fs::read_to_string(&toml_path)
        .with_context(|| format!("reading {}", toml_path.display()))?;

    let mut doc: toml::Value = content
        .parse()
        .with_context(|| format!("parsing {}", toml_path.display()))?;

    ensure_messages_table(&mut doc, crate_name)?;

    let msg_array = doc["messages"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("[[messages]] is not an array"))?;

    let msg_entry = msg_array
        .get_mut(0)
        .ok_or_else(|| anyhow::anyhow!("[[messages]] array is empty"))?;
    let msg_table = msg_entry
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("messages entry is not a table"))?;

    if has_variant(msg_table, variant_name) {
        bail!(
            "variant '{}' already exists in messages",
            variant_name
        );
    }

    let mut variant = toml::map::Map::new();
    variant.insert(
        "name".into(),
        toml::Value::String(variant_name.into()),
    );

    if !fields.is_empty() {
        let mut fields_arr = Vec::new();
        for (fname, fty) in fields {
            let mut field = toml::map::Map::new();
            field.insert("name".into(), toml::Value::String(fname));
            field.insert("ty".into(), toml::Value::String(fty));
            fields_arr.push(toml::Value::Table(field));
        }
        variant.insert("fields".into(), toml::Value::Array(fields_arr));
    }

    let variants_entry = msg_table
        .entry("variants")
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    variants_entry
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("messages.variants is not an array"))?
        .push(toml::Value::Table(variant));

    let output = toml::to_string_pretty(&doc)?;
    fs::write(&toml_path, &output)?;

    println!(
        "Added variant '{}' to {}",
        variant_name,
        toml_path.display()
    );
    Ok(())
}

pub fn remove_message(crate_name: &str, variant_name: &str) -> anyhow::Result<()> {
    let toml_path = blox_toml_path(crate_name);
    let content = fs::read_to_string(&toml_path)
        .with_context(|| format!("reading {}", toml_path.display()))?;

    let mut doc: toml::Value = content
        .parse()
        .with_context(|| format!("parsing {}", toml_path.display()))?;

    let msg_array = doc
        .get_mut("messages")
        .ok_or_else(|| anyhow::anyhow!("no [[messages]] table found"))?
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("[[messages]] is not an array"))?;

    let mut found = false;
    for msg_entry in msg_array.iter_mut() {
        if let Some(table) = msg_entry.as_table_mut() {
            if let Some(variants) = table.get_mut("variants") {
                if let Some(variants_arr) = variants.as_array_mut() {
                    let before = variants_arr.len();
                    variants_arr.retain(|v| {
                        v.get("name").and_then(|n| n.as_str()) != Some(variant_name)
                    });
                    if variants_arr.len() < before {
                        found = true;
                    }
                }
            }
        }
    }

    if !found {
        bail!(
            "variant '{}' not found in any messages table",
            variant_name
        );
    }

    let output = toml::to_string_pretty(&doc)?;
    fs::write(&toml_path, &output)?;

    println!(
        "Removed variant '{}' from {}",
        variant_name,
        toml_path.display()
    );
    Ok(())
}

fn blox_toml_path(crate_name: &str) -> PathBuf {
    Path::new("crates/messages").join(crate_name).join("blox.toml")
}

fn crate_name_to_msg_name(crate_name: &str) -> String {
    let base = crate_name
        .strip_suffix("-messages")
        .unwrap_or(crate_name);
    base.split(|c: char| c == '-' || c == '_')
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

fn ensure_messages_table(doc: &mut toml::Value, crate_name: &str) -> anyhow::Result<()> {
    if doc.get("messages").is_none() {
        let msg_name = crate_name_to_msg_name(crate_name);
        let mut table = toml::map::Map::new();
        table.insert("name".into(), toml::Value::String(msg_name));
        table.insert("visibility".into(), toml::Value::String("pub".into()));
        doc.as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("document root is not a table"))?
            .insert("messages".into(), toml::Value::Array(vec![toml::Value::Table(table)]));
    }
    Ok(())
}

fn has_variant(msg_table: &toml::map::Map<String, toml::Value>, name: &str) -> bool {
    msg_table
        .get("variants")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().any(|v| {
                v.get("name").and_then(|n| n.as_str()) == Some(name)
            })
        })
        .unwrap_or(false)
}
