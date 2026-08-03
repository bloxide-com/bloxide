// Copyright 2025 Bloxide, all rights reserved
//! Manage `system.toml` wiring manifests (#117): actors, supervision, policies,
//! and constructor injections.

use std::path::PathBuf;

use anyhow::bail;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::toml_helpers::{load_toml, save_toml, system_toml_path_for_app};

fn load_app(app_name: &str) -> anyhow::Result<(PathBuf, DocumentMut)> {
    let path = system_toml_path_for_app(app_name);
    if !path.exists() {
        return Err(crate::exit::not_found(format!(
            "system.toml not found for app '{}' at {}",
            app_name,
            path.display()
        ))
        .into());
    }
    let doc = load_toml(&path)?;
    Ok((path, doc))
}

fn actors_array_mut(doc: &mut DocumentMut) -> anyhow::Result<&mut ArrayOfTables> {
    if doc.get("actors").is_none() {
        doc["actors"] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    doc.get_mut("actors")
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("[[actors]] is not an array of tables"))
}

fn supervision_array_mut(doc: &mut DocumentMut) -> anyhow::Result<&mut ArrayOfTables> {
    if doc.get("supervision").is_none() {
        doc["supervision"] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    doc.get_mut("supervision")
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or_else(|| anyhow::anyhow!("[[supervision]] is not an array of tables"))
}

fn actor_name_of(t: &Table) -> Option<&str> {
    t.get("name").and_then(|v| v.as_str())
}

fn string_array(items: &[String]) -> Item {
    let mut arr = Array::new();
    for i in items {
        arr.push(i.as_str());
    }
    toml_edit::value(arr)
}

fn generate_hint() {
    println!("Run `cargo blox generate` to regenerate main.rs and Cargo.toml.");
}

// ── add-actor / remove-actor ────────────────────────────────────────────────

pub fn add_actor(
    app_name: &str,
    name: &str,
    blox: &str,
    impl_crate: Option<&str>,
    kind: Option<&str>,
    features: Vec<String>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    let (path, mut doc) = load_app(app_name)?;
    let actors = actors_array_mut(&mut doc)?;

    if actors.iter().any(|t| actor_name_of(t) == Some(name)) {
        if if_not_exists {
            return Ok(());
        }
        return Err(crate::exit::conflict(format!(
            "actor '{}' already exists in {}",
            name, app_name
        ))
        .into());
    }

    let mut t = Table::new();
    t["name"] = toml_edit::value(name);
    t["blox"] = toml_edit::value(blox);
    if let Some(ic) = impl_crate {
        t["impl_crate"] = toml_edit::value(ic);
    }
    if let Some(k) = kind {
        t["kind"] = toml_edit::value(k);
    }
    if !features.is_empty() {
        t["features"] = string_array(&features);
    }
    actors.push(t);

    save_toml(&path, &doc)?;
    println!("Added actor '{}' ({}) to {}", name, blox, app_name);
    generate_hint();
    Ok(())
}

pub fn remove_actor(app_name: &str, name: &str) -> anyhow::Result<()> {
    let (path, mut doc) = load_app(app_name)?;

    let exists = {
        let actors = actors_array_mut(&mut doc)?;
        let exists = actors.iter().any(|t| actor_name_of(t) == Some(name));
        if exists {
            let indices: Vec<usize> = actors
                .iter()
                .enumerate()
                .filter_map(|(i, t)| (actor_name_of(t) == Some(name)).then_some(i))
                .collect();
            for i in indices.into_iter().rev() {
                actors.remove(i);
            }
        }
        exists
    };
    if !exists {
        return Err(
            crate::exit::not_found(format!("actor '{}' not found in {}", name, app_name)).into(),
        );
    }

    // Clean up dangling references: supervision children + policies.
    let sup = supervision_array_mut(&mut doc)?;
    for entry in sup.iter_mut() {
        if let Some(children) = entry.get_mut("children").and_then(|c| c.as_array_mut()) {
            let mut cleaned = Array::new();
            for c in children.iter() {
                if c.as_str() != Some(name) {
                    cleaned.push(c.clone());
                }
            }
            *children = cleaned;
        }
        if let Some(policies) = entry.get_mut("policies").and_then(|p| p.as_table_mut()) {
            policies.remove(name);
        }
    }

    save_toml(&path, &doc)?;
    println!("Removed actor '{}' from {}", name, app_name);
    generate_hint();
    Ok(())
}

// ── add-supervision / remove-supervision ────────────────────────────────────

pub fn add_supervision(
    app_name: &str,
    supervisor: &str,
    strategy: &str,
    children: Vec<String>,
    if_not_exists: bool,
) -> anyhow::Result<()> {
    const STRATEGIES: [&str; 2] = ["when_any_done", "when_all_done"];
    if !STRATEGIES.contains(&strategy) {
        bail!(
            "unknown strategy '{}' — expected one of: {}",
            strategy,
            STRATEGIES.join(", ")
        );
    }

    let (path, mut doc) = load_app(app_name)?;
    let sup = supervision_array_mut(&mut doc)?;

    if sup
        .iter()
        .any(|t| t.get("supervisor").and_then(|v| v.as_str()) == Some(supervisor))
    {
        if if_not_exists {
            return Ok(());
        }
        return Err(crate::exit::conflict(format!(
            "supervision for '{}' already exists in {}",
            supervisor, app_name
        ))
        .into());
    }

    let mut t = Table::new();
    t["supervisor"] = toml_edit::value(supervisor);
    t["strategy"] = toml_edit::value(strategy);
    t["children"] = string_array(&children);
    sup.push(t);

    save_toml(&path, &doc)?;
    println!(
        "Added supervision ({}, {}) with {} children to {}",
        supervisor,
        strategy,
        children.len(),
        app_name
    );
    generate_hint();
    Ok(())
}

pub fn remove_supervision(app_name: &str, supervisor: &str) -> anyhow::Result<()> {
    let (path, mut doc) = load_app(app_name)?;
    let sup = supervision_array_mut(&mut doc)?;

    let exists = sup
        .iter()
        .any(|t| t.get("supervisor").and_then(|v| v.as_str()) == Some(supervisor));
    if !exists {
        return Err(crate::exit::not_found(format!(
            "supervision for '{}' not found in {}",
            supervisor, app_name
        ))
        .into());
    }

    let indices: Vec<usize> = sup
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            (t.get("supervisor").and_then(|v| v.as_str()) == Some(supervisor)).then_some(i)
        })
        .collect();
    for i in indices.into_iter().rev() {
        sup.remove(i);
    }

    save_toml(&path, &doc)?;
    println!("Removed supervision '{}' from {}", supervisor, app_name);
    generate_hint();
    Ok(())
}

// ── set-policy ──────────────────────────────────────────────────────────────

pub fn set_policy(
    app_name: &str,
    actor: &str,
    restart_max: Option<u32>,
    stop: bool,
) -> anyhow::Result<()> {
    if restart_max.is_none() && !stop {
        bail!("set-policy requires --restart-max <n> or --stop");
    }

    let (path, mut doc) = load_app(app_name)?;
    let sup = supervision_array_mut(&mut doc)?;
    // With multiple [[supervision]] groups, edit the one whose `children`
    // list contains the actor — not blindly the first entry.
    let entry = if sup.len() == 1 {
        sup.iter_mut().next().unwrap()
    } else {
        sup.iter_mut()
            .find(|t| {
                t.get("children")
                    .and_then(|c| c.as_array())
                    .map(|children| children.iter().any(|a| a.as_str() == Some(actor)))
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no [[supervision]] group in {} lists '{}' among its children",
                    app_name,
                    actor
                )
            })?
    };

    if entry.get("policies").is_none() {
        entry["policies"] = Item::Table(Table::new());
    }
    let policies = entry
        .get_mut("policies")
        .and_then(|p| p.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!("policies is not a table"))?;

    let mut policy = InlineTable::new();
    if let Some(max) = restart_max {
        let mut restart = InlineTable::new();
        restart.insert("max", Value::from(max as i64));
        policy.insert("restart", Value::InlineTable(restart));
    }
    if stop {
        policy.insert("stop", Value::from(true));
    }
    policies[actor] = toml_edit::value(policy);

    save_toml(&path, &doc)?;
    println!("Set policy for '{}' in {}", actor, app_name);
    generate_hint();
    Ok(())
}

// ── add-injection ───────────────────────────────────────────────────────────

/// Parse a `--from` spec into an InjectSource table:
/// - `self` → `{ source = "self" }`
/// - `self_secondary[:<index>]` → `{ source = "self_secondary", index = n }`
/// - `actor:<name>[:<field>]` → `{ source = "actor", actor = name, field = f }`
/// - `factory:<crate>:<function>` → `{ source = "factory", crate = c, function = f }`
fn parse_inject_source(spec: &str) -> anyhow::Result<InlineTable> {
    let parts: Vec<&str> = spec.split(':').collect();
    let mut t = InlineTable::new();
    match parts.as_slice() {
        ["self"] => {
            t.insert("source", Value::from("self"));
        }
        ["self_secondary"] => {
            t.insert("source", Value::from("self_secondary"));
        }
        ["self_secondary", index] => {
            let idx: i64 = index
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid self_secondary index '{}'", index))?;
            t.insert("source", Value::from("self_secondary"));
            t.insert("index", Value::from(idx));
        }
        ["actor", actor] => {
            t.insert("source", Value::from("actor"));
            t.insert("actor", Value::from(*actor));
        }
        ["actor", actor, field] => {
            t.insert("source", Value::from("actor"));
            t.insert("actor", Value::from(*actor));
            t.insert("field", Value::from(*field));
        }
        ["factory", crate_name, function] => {
            t.insert("source", Value::from("factory"));
            t.insert("crate", Value::from(*crate_name));
            t.insert("function", Value::from(*function));
        }
        _ => bail!(
            "invalid --from '{}' — expected: self | self_secondary[:idx] | actor:<name>[:<field>] | factory:<crate>:<fn>",
            spec
        ),
    }
    Ok(t)
}

pub fn add_injection(app_name: &str, actor: &str, field: &str, from: &str) -> anyhow::Result<()> {
    let (path, mut doc) = load_app(app_name)?;
    let source = parse_inject_source(from)?;

    let actors = actors_array_mut(&mut doc)?;
    let entry = actors
        .iter_mut()
        .find(|t| actor_name_of(t) == Some(actor))
        .ok_or_else(|| {
            anyhow::Error::from(crate::exit::not_found(format!(
                "actor '{}' not found in {}",
                actor, app_name
            )))
        })?;

    if entry.get("inject").is_none() {
        entry["inject"] = Item::Table(Table::new());
    }
    let inject = entry
        .get_mut("inject")
        .and_then(|i| i.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!("inject is not a table"))?;
    inject[field] = toml_edit::value(source);

    save_toml(&path, &doc)?;
    println!(
        "Added injection {}.{} <- {} in {}",
        actor, field, from, app_name
    );
    generate_hint();
    Ok(())
}
