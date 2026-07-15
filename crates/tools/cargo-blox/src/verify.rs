// Copyright 2025 Bloxide, all rights reserved
//! Round-trip verification: blox.toml → codegen → viz-export → JSON → compare.
//!
//! Runs the full pipeline on every `blox.toml` in the workspace and verifies
//! that no data is lost between the declarative spec and the visualizer model.

use bloxide_codegen::generate_from_toml;
use bloxide_codegen::schema::BloxConfig;
use bloxide_viz_export::{export_workspace, model::BloxSpec};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn verify(workspace: Option<PathBuf>) -> anyhow::Result<()> {
    let root = workspace.unwrap_or_else(|| {
        let manifest_dir =
            PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
        find_workspace_root(&manifest_dir).unwrap_or(manifest_dir)
    });

    println!(
        "bloxide: verifying round-trip for workspace at {}",
        root.display()
    );

    // Step 1: Find all blox.toml files
    let tomls = find_blox_tomls(&root);
    if tomls.is_empty() {
        anyhow::bail!("no blox.toml files found in {}", root.display());
    }
    println!("bloxide: found {} blox.toml file(s)", tomls.len());

    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0;

    // Pre-parse all configs, keyed by actor name for matching to specs
    let mut configs_by_actor: HashMap<String, BloxConfig> = HashMap::new();

    // Step 2: Parse + codegen each blox.toml
    for toml_path in &tomls {
        let rel = toml_path.strip_prefix(&root).unwrap_or(toml_path).display();

        let content = match fs::read_to_string(toml_path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{}: read error: {}", rel, e));
                continue;
            }
        };

        let config: BloxConfig = match toml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{}: parse error: {}", rel, e));
                continue;
            }
        };

        // Codegen must produce output
        match generate_from_toml(toml_path) {
            Ok(files) => {
                if files.is_empty() && config.actor.is_some() {
                    errors.push(format!("{}: codegen produced no files", rel));
                }
            }
            Err(e) => {
                errors.push(format!("{}: codegen error: {}", rel, e));
                continue;
            }
        }

        // Store for later comparison
        if let Some(actor) = &config.actor {
            configs_by_actor.insert(actor.name.clone(), config.clone());
        }

        checked += 1;
    }
    println!("bloxide: codegen verified for {} blox.toml(s)", checked);

    // Step 3: viz-export the entire workspace
    let specs = match export_workspace(&root) {
        Ok(s) => s,
        Err(e) => {
            anyhow::bail!("viz-export failed: {}", e);
        }
    };
    println!("bloxide: viz-export produced {} spec(s)", specs.len());

    // Step 4: JSON round-trip on every spec
    for spec in &specs {
        let json = serde_json::to_string_pretty(spec)?;
        let back: BloxSpec = serde_json::from_str(&json)?;
        if spec != &back {
            errors.push(format!("spec '{}': JSON round-trip data loss", spec.name));
        }
    }
    println!(
        "bloxide: JSON round-trip verified for {} spec(s)",
        specs.len()
    );

    // Step 5: Verify states, transitions, context, wiring are present
    for spec in &specs {
        if spec.states.is_empty() {
            errors.push(format!("spec '{}': no states", spec.name));
        }

        // Match this spec to its original BloxConfig
        let config = match configs_by_actor.get(&spec.name) {
            Some(c) => c,
            None => continue, // Not an actor blox, skip
        };

        // Verify states
        if let Some(topo) = &config.topology {
            for state in &topo.states {
                if !spec.states.iter().any(|s| s.name == state.name) {
                    errors.push(format!(
                        "spec '{}': state '{}' missing",
                        spec.name, state.name
                    ));
                }
            }

            // Verify transitions
            for trans in &topo.transitions {
                let (msg_set, variant) = parse_event_pattern(&trans.event);
                let full_event = format!("{}::{}", msg_set, variant);
                let found = spec.handlers.iter().any(|h| {
                    h.state == trans.state
                        && h.event == full_event
                        && matches!(h.source, bloxide_viz_export::model::HandlerSource::Explicit)
                });
                if !found {
                    errors.push(format!(
                        "spec '{}': transition {} in state '{}' missing",
                        spec.name, full_event, trans.state
                    ));
                }
            }
        }

        // Verify context
        if let Some(ctx) = &config.context {
            let exported_ctx = match &spec.context {
                Some(c) => c,
                None => {
                    errors.push(format!("spec '{}': context missing", spec.name));
                    continue;
                }
            };
            if exported_ctx.struct_name != ctx.name {
                errors.push(format!(
                    "spec '{}': context name '{}' != '{}'",
                    spec.name, exported_ctx.struct_name, ctx.name
                ));
            }
            for field in &ctx.fields {
                if !exported_ctx.fields.iter().any(|f| f.name == field.name) {
                    errors.push(format!(
                        "spec '{}': context field '{}' missing",
                        spec.name, field.name
                    ));
                }
            }
        }

        // Verify wiring
        if let Some(wiring) = &config.wiring {
            if !wiring.actors.is_empty() {
                let exported_wiring = match &spec.wiring {
                    Some(w) => w,
                    None => {
                        errors.push(format!("spec '{}': wiring missing", spec.name));
                        continue;
                    }
                };
                if exported_wiring.runtime != wiring.runtime {
                    errors.push(format!(
                        "spec '{}': wiring runtime '{}' != '{}'",
                        spec.name, exported_wiring.runtime, wiring.runtime
                    ));
                }
                for actor in &wiring.actors {
                    if !exported_wiring.actors.iter().any(|a| a.name == actor.name) {
                        errors.push(format!(
                            "spec '{}': wiring actor '{}' missing",
                            spec.name, actor.name
                        ));
                    }
                }
                for conn in &wiring.connections {
                    if !exported_wiring.connections.iter().any(|c| {
                        c.from == conn.from && c.to == conn.to && c.message == conn.message
                    }) {
                        errors.push(format!(
                            "spec '{}': wiring connection {} → {} ({}) missing",
                            spec.name, conn.from, conn.to, conn.message
                        ));
                    }
                }
            }
        }
    }

    // Step 6: Report
    if errors.is_empty() {
        println!(
            "bloxide: round-trip verification PASSED — {} blox.toml(s), {} spec(s), 0 errors",
            tomls.len(),
            specs.len()
        );
        Ok(())
    } else {
        eprintln!(
            "bloxide: round-trip verification FAILED — {} error(s):",
            errors.len()
        );
        for err in &errors {
            eprintln!("  - {}", err);
        }
        anyhow::bail!(
            "round-trip verification failed with {} error(s)",
            errors.len()
        );
    }
}

fn find_blox_tomls(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            // Skip target/ and hidden directories; only match files named blox.toml
            e.depth() > 0
                && e.file_type().is_file()
                && !e
                    .path()
                    .components()
                    .any(|c| c.as_os_str() == std::ffi::OsStr::new("target"))
                && e.file_name() == "blox.toml"
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return Some(current.to_path_buf());
                }
            }
        }
        current = current.parent()?;
    }
}

fn parse_event_pattern(pattern: &str) -> (String, String) {
    let pattern = pattern.trim();
    let clean = if let Some(pos) = pattern.find('(') {
        &pattern[..pos]
    } else {
        pattern
    };
    if let Some(pos) = clean.find("::") {
        let message_set = clean[..pos].trim().to_string();
        let variant = clean[pos + 2..].trim().to_string();
        (message_set, variant)
    } else {
        ("Unknown".to_string(), clean.to_string())
    }
}
