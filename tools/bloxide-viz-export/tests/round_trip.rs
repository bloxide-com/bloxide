// Copyright 2025 Bloxide, all rights reserved
//! Round-trip verification: blox.toml → codegen → viz-export → JSON → visualizer model → compare.
//!
//! This integration test verifies that every `blox.toml` in the repository can:
//! 1. Be parsed as a `BloxConfig`
//! 2. Produce codegen output (the codegen itself is already tested elsewhere;
//!    here we verify the codegen files are produced without error)
//! 3. Be exported by viz-export into a `BloxSpec`
//! 4. Serialize to JSON and deserialize back without data loss
//! 5. Have all states, transitions, context, and wiring present in the exported model
//! 6. Round-trip back to the original `BloxConfig` fields with no data loss

use bloxide_codegen::generate_from_toml;
use bloxide_codegen::schema::BloxConfig;
use bloxide_viz_export::{export_workspace, model::BloxSpec};
use std::fs;
use std::path::{Path, PathBuf};

/// Locate the workspace root from the test crate's manifest dir.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Recursively find all `blox.toml` files under a path, skipping `target/`
/// and hidden directories.
fn find_blox_tomls(root: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    find_blox_tomls_recursive(root, 0, 8, &mut results);
    results.sort();
    results
}

fn find_blox_tomls_recursive(
    path: &Path,
    depth: usize,
    max_depth: usize,
    results: &mut Vec<PathBuf>,
) {
    if depth > max_depth {
        return;
    }
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "target" {
                continue;
            }
        }
        if entry_path.is_dir() {
            find_blox_tomls_recursive(&entry_path, depth + 1, max_depth, results);
        } else if entry_path.file_name() == Some(std::ffi::OsStr::new("blox.toml")) {
            results.push(entry_path);
        }
    }
}

/// Parse a blox.toml file into a BloxConfig.
fn parse_blox_toml(path: &Path) -> (String, BloxConfig) {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    let config: BloxConfig = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", path.display(), e));
    let crate_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    (crate_name, config)
}

// ---------------------------------------------------------------------------
// Step 1: Every blox.toml parses successfully
// ---------------------------------------------------------------------------

#[test]
fn test_all_blox_tomls_parse() {
    let ws = workspace_root();
    let tomls = find_blox_tomls(&ws);
    assert!(
        tomls.len() >= 7,
        "expected at least 7 blox.toml files, found {}: {:?}",
        tomls.len(),
        tomls
    );

    for toml_path in &tomls {
        let (_name, config) = parse_blox_toml(toml_path);
        // Every blox.toml should have at least one recognized section
        assert!(
            config.actor.is_some()
                || config.topology.is_some()
                || config.messages.is_some()
                || config.mailboxes.is_some()
                || config.wiring.is_some(),
            "blox.toml at {} has no recognized sections",
            toml_path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Step 2: Codegen produces output for every blox.toml
// ---------------------------------------------------------------------------

#[test]
fn test_all_blox_tomls_codegen() {
    let ws = workspace_root();
    let tomls = find_blox_tomls(&ws);

    for toml_path in &tomls {
        let files = generate_from_toml(toml_path)
            .unwrap_or_else(|e| panic!("codegen failed for {}: {}", toml_path.display(), e));
        assert!(
            !files.is_empty(),
            "codegen produced no files for {}",
            toml_path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Step 3: viz-export produces a BloxSpec for every blox.toml
// ---------------------------------------------------------------------------

#[test]
fn test_viz_export_covers_all_blox_tomls() {
    let ws = workspace_root();
    let tomls = find_blox_tomls(&ws);
    let specs = export_workspace(&ws).expect("export should succeed");

    // Every blox.toml that has [actor] or [topology] should produce a spec
    let actor_tomls: Vec<_> = tomls
        .iter()
        .filter(|p| {
            let content = fs::read_to_string(p).unwrap_or_default();
            content.contains("[actor]") || content.contains("[topology]")
        })
        .collect();

    assert!(
        specs.len() >= actor_tomls.len(),
        "viz-export produced {} specs but found {} actor/topology blox.tomls",
        specs.len(),
        actor_tomls.len()
    );
}

// ---------------------------------------------------------------------------
// Step 4: JSON round-trip — every BloxSpec serializes and deserializes
// ---------------------------------------------------------------------------

#[test]
fn test_json_round_trip_all_specs() {
    let ws = workspace_root();
    let specs = export_workspace(&ws).expect("export should succeed");

    for spec in &specs {
        let json = serde_json::to_string_pretty(spec)
            .unwrap_or_else(|e| panic!("failed to serialize {}: {}", spec.name, e));
        let back: BloxSpec = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("failed to deserialize {}: {}", spec.name, e));
        assert_eq!(spec, &back, "JSON round-trip mismatch for {}", spec.name);
    }
}

// ---------------------------------------------------------------------------
// Step 5: Verify states, transitions, context, and wiring are present
// ---------------------------------------------------------------------------

#[test]
fn test_all_specs_have_states() {
    let ws = workspace_root();
    let specs = export_workspace(&ws).expect("export should succeed");

    for spec in &specs {
        assert!(
            !spec.states.is_empty(),
            "spec '{}' has no states",
            spec.name
        );
    }
}

#[test]
fn test_explicit_transitions_preserved() {
    let ws = workspace_root();
    let tomls = find_blox_tomls(&ws);

    for toml_path in &tomls {
        let (_name, config) = parse_blox_toml(toml_path);

        // If the TOML has declarative transitions, they must appear in the
        // exported spec's handlers (as Explicit source).
        if let Some(topo) = &config.topology {
            if !topo.transitions.is_empty() {
                let specs = export_workspace(&ws).expect("export should succeed");
                let actor_name = config
                    .actor
                    .as_ref()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| {
                        toml_path
                            .parent()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string()
                    });

                let spec = specs.iter().find(|s| s.name == actor_name);
                if let Some(spec) = spec {
                    let explicit_count = spec
                        .handlers
                        .iter()
                        .filter(|h| {
                            matches!(h.source, bloxide_viz_export::model::HandlerSource::Explicit)
                        })
                        .count();

                    assert!(
                        explicit_count >= topo.transitions.len(),
                        "spec '{}' has {} explicit handlers but TOML declares {} transitions",
                        spec.name,
                        explicit_count,
                        topo.transitions.len()
                    );
                }
            }
        }
    }
}

#[test]
fn test_context_preserved() {
    let ws = workspace_root();
    let tomls = find_blox_tomls(&ws);
    let specs = export_workspace(&ws).expect("export should succeed");

    for toml_path in &tomls {
        let (_name, config) = parse_blox_toml(toml_path);

        if let Some(ctx) = &config.context {
            let actor_name = config
                .actor
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| {
                    toml_path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string()
                });

            let spec = specs
                .iter()
                .find(|s| s.name == actor_name)
                .unwrap_or_else(|| panic!("spec '{}' not found in export", actor_name));

            let exported_ctx = spec
                .context
                .as_ref()
                .unwrap_or_else(|| panic!("spec '{}' has no context", spec.name));

            assert_eq!(
                exported_ctx.struct_name, ctx.name,
                "context struct name mismatch for {}",
                spec.name
            );

            // Verify each field from the TOML appears in the exported context
            for field in &ctx.fields {
                assert!(
                    exported_ctx.fields.iter().any(|f| f.name == field.name),
                    "field '{}' missing from exported context for {}",
                    field.name,
                    spec.name
                );
            }
        }
    }
}

#[test]
fn test_wiring_preserved() {
    let ws = workspace_root();
    let tomls = find_blox_tomls(&ws);
    let specs = export_workspace(&ws).expect("export should succeed");

    for toml_path in &tomls {
        let (_name, config) = parse_blox_toml(toml_path);

        if let Some(wiring) = &config.wiring {
            if !wiring.actors.is_empty() {
                // Find the spec that has wiring — it might not match by actor
                // name since wiring is a separate concern. Look for any spec
                // with wiring that matches the runtime.
                let spec_with_wiring = specs.iter().find(|s| s.wiring.is_some());

                if let Some(spec) = spec_with_wiring {
                    let exported_wiring = spec.wiring.as_ref().unwrap();

                    assert_eq!(
                        exported_wiring.runtime, wiring.runtime,
                        "wiring runtime mismatch"
                    );

                    // Verify all actors are present
                    for actor in &wiring.actors {
                        assert!(
                            exported_wiring.actors.iter().any(|a| a.name == actor.name),
                            "wiring actor '{}' missing from export",
                            actor.name
                        );
                    }

                    // Verify all connections are present
                    for conn in &wiring.connections {
                        assert!(
                            exported_wiring.connections.iter().any(|c| {
                                c.from == conn.from && c.to == conn.to && c.message == conn.message
                            }),
                            "wiring connection {} → {} ({}) missing from export",
                            conn.from,
                            conn.to,
                            conn.message
                        );
                    }

                    // Verify all supervisors are present
                    for sup in &wiring.supervisors {
                        assert!(
                            exported_wiring
                                .supervisors
                                .iter()
                                .any(|s| { s.name == sup.name && s.strategy == sup.strategy }),
                            "wiring supervisor '{}' missing from export",
                            sup.name
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Step 6: Full round-trip — BloxConfig → BloxSpec → JSON → BloxSpec → compare
// ---------------------------------------------------------------------------

#[test]
fn test_full_round_trip_no_data_loss() {
    let ws = workspace_root();
    let tomls = find_blox_tomls(&ws);
    let specs = export_workspace(&ws).expect("export should succeed");

    for toml_path in &tomls {
        let (_crate_name, config) = parse_blox_toml(toml_path);

        // Only test blox.tomls that have [actor] (i.e., they are blox crates,
        // not just message crates)
        if config.actor.is_none() {
            continue;
        }

        let actor_name = config.actor.as_ref().unwrap().name.clone();

        let spec = specs
            .iter()
            .find(|s| s.name == actor_name)
            .unwrap_or_else(|| {
                panic!(
                    "spec '{}' not found in export (from {})",
                    actor_name,
                    toml_path.display()
                )
            });

        // --- States round-trip ---
        if let Some(topo) = &config.topology {
            for state in &topo.states {
                let exported = spec
                    .states
                    .iter()
                    .find(|s| s.name == state.name)
                    .unwrap_or_else(|| {
                        panic!(
                            "state '{}' missing from export for {}",
                            state.name, spec.name
                        )
                    });

                // Verify kind
                let expected_kind = if state.error.unwrap_or(false) {
                    bloxide_viz_export::model::StateKind::Error
                } else if state.terminal.unwrap_or(false) {
                    bloxide_viz_export::model::StateKind::Terminal
                } else if state.composite.unwrap_or(false) {
                    bloxide_viz_export::model::StateKind::Composite
                } else {
                    bloxide_viz_export::model::StateKind::Leaf
                };
                assert_eq!(
                    exported.kind, expected_kind,
                    "state '{}' kind mismatch for {}",
                    state.name, spec.name
                );

                // Verify parent
                assert_eq!(
                    exported.parent, state.parent,
                    "state '{}' parent mismatch for {}",
                    state.name, spec.name
                );
            }
        }

        // --- Transitions round-trip ---
        if let Some(topo) = &config.topology {
            for trans in &topo.transitions {
                // Each transition should produce at least one handler
                let (msg_set, variant) = parse_event_pattern(&trans.event);
                let full_event = format!("{}::{}", msg_set, variant);

                let handler = spec.handlers.iter().find(|h| {
                    h.state == trans.state
                        && h.event == full_event
                        && h.actions == trans.actions
                        && matches!(h.source, bloxide_viz_export::model::HandlerSource::Explicit)
                });

                assert!(
                    handler.is_some(),
                    "transition {} in state {} missing from export for {}",
                    full_event,
                    trans.state,
                    spec.name
                );

                if let Some(h) = handler {
                    // Verify actions
                    assert_eq!(
                        h.actions, trans.actions,
                        "actions mismatch for {}::{} in {}",
                        trans.state, full_event, spec.name
                    );

                    // Verify target (when no guards)
                    if trans.guards.is_empty() {
                        let expected = parse_target(&trans.target);
                        assert_eq!(
                            h.target, expected,
                            "target mismatch for {}::{} in {}",
                            trans.state, full_event, spec.name
                        );
                    } else {
                        // Verify guard branches
                        assert_eq!(
                            h.guard.branches.len(),
                            trans.guards.len(),
                            "guard branch count mismatch for {}::{} in {}",
                            trans.state,
                            full_event,
                            spec.name
                        );
                        for (i, g) in trans.guards.iter().enumerate() {
                            assert_eq!(
                                h.guard.branches[i].condition, g.condition,
                                "guard {} condition mismatch for {}::{} in {}",
                                i, trans.state, full_event, spec.name
                            );
                            let expected_target = parse_target(&g.target);
                            assert_eq!(
                                h.guard.branches[i].target, expected_target,
                                "guard {} target mismatch for {}::{} in {}",
                                i, trans.state, full_event, spec.name
                            );
                        }
                    }
                }
            }
        }

        // --- Entry/exit round-trip ---
        if let Some(topo) = &config.topology {
            for entry in &topo.entry {
                let ee = spec.entry_exit.get(&entry.state);
                assert!(
                    ee.is_some(),
                    "entry actions for state '{}' missing from export for {}",
                    entry.state,
                    spec.name
                );
                if let Some(ee) = ee {
                    assert_eq!(
                        ee.on_entry, entry.actions,
                        "on_entry mismatch for state '{}' in {}",
                        entry.state, spec.name
                    );
                }
            }

            for exit in &topo.exit {
                let ee = spec.entry_exit.get(&exit.state);
                assert!(
                    ee.is_some(),
                    "exit actions for state '{}' missing from export for {}",
                    exit.state,
                    spec.name
                );
                if let Some(ee) = ee {
                    assert_eq!(
                        ee.on_exit, exit.actions,
                        "on_exit mismatch for state '{}' in {}",
                        exit.state, spec.name
                    );
                }
            }
        }

        // --- Context round-trip ---
        if let Some(ctx) = &config.context {
            let exported_ctx = spec
                .context
                .as_ref()
                .unwrap_or_else(|| panic!("context missing from export for {}", spec.name));

            assert_eq!(
                exported_ctx.struct_name, ctx.name,
                "context struct name mismatch for {}",
                spec.name
            );

            assert_eq!(
                exported_ctx.fields.len(),
                ctx.fields.len(),
                "context field count mismatch for {} (expected {}, got {})",
                spec.name,
                ctx.fields.len(),
                exported_ctx.fields.len()
            );

            for (i, field) in ctx.fields.iter().enumerate() {
                assert_eq!(
                    exported_ctx.fields[i].name, field.name,
                    "context field {} name mismatch for {}",
                    i, spec.name
                );
                assert_eq!(
                    exported_ctx.fields[i].ty, field.ty,
                    "context field '{}' type mismatch for {}",
                    field.name, spec.name
                );

                // Verify delegates annotation
                let expected_delegates: Vec<String> = field
                    .delegates
                    .as_ref()
                    .map(|ds| ds.iter().map(|d| format!("#[delegates({})]", d)).collect())
                    .unwrap_or_default();
                assert_eq!(
                    exported_ctx.fields[i].annotations, expected_delegates,
                    "context field '{}' delegates mismatch for {}",
                    field.name, spec.name
                );
            }
        }

        // --- Wiring round-trip ---
        if let Some(wiring) = &config.wiring {
            if !wiring.actors.is_empty() {
                let exported_wiring = spec
                    .wiring
                    .as_ref()
                    .unwrap_or_else(|| panic!("wiring missing from export for {}", spec.name));

                assert_eq!(
                    exported_wiring.runtime, wiring.runtime,
                    "wiring runtime mismatch for {}",
                    spec.name
                );

                assert_eq!(
                    exported_wiring.actors.len(),
                    wiring.actors.len(),
                    "wiring actor count mismatch for {}",
                    spec.name
                );

                for (i, actor) in wiring.actors.iter().enumerate() {
                    assert_eq!(exported_wiring.actors[i].blox, actor.blox);
                    assert_eq!(exported_wiring.actors[i].name, actor.name);
                    assert_eq!(exported_wiring.actors[i].behavior, actor.behavior);
                    assert_eq!(
                        exported_wiring.actors[i].behavior_traits,
                        actor.behavior_traits
                    );
                }

                assert_eq!(
                    exported_wiring.connections.len(),
                    wiring.connections.len(),
                    "wiring connection count mismatch for {}",
                    spec.name
                );

                for (i, conn) in wiring.connections.iter().enumerate() {
                    assert_eq!(exported_wiring.connections[i].from, conn.from);
                    assert_eq!(exported_wiring.connections[i].to, conn.to);
                    assert_eq!(exported_wiring.connections[i].message, conn.message);
                    assert_eq!(
                        exported_wiring.connections[i].channel_capacity,
                        conn.channel_capacity
                    );
                }

                assert_eq!(
                    exported_wiring.supervisors.len(),
                    wiring.supervisors.len(),
                    "wiring supervisor count mismatch for {}",
                    spec.name
                );

                for (i, sup) in wiring.supervisors.iter().enumerate() {
                    assert_eq!(exported_wiring.supervisors[i].name, sup.name);
                    assert_eq!(exported_wiring.supervisors[i].strategy, sup.strategy);
                    assert_eq!(
                        exported_wiring.supervisors[i].children.len(),
                        sup.children.len()
                    );
                    for (j, child) in sup.children.iter().enumerate() {
                        assert_eq!(
                            exported_wiring.supervisors[i].children[j].actor,
                            child.actor
                        );
                        assert_eq!(
                            exported_wiring.supervisors[i].children[j].restart_max,
                            child.restart_max
                        );
                    }
                }
            }
        }

        // --- JSON round-trip within the full test ---
        let json = serde_json::to_string_pretty(spec)
            .unwrap_or_else(|e| panic!("JSON serialize failed for {}: {}", spec.name, e));
        let deserialized: BloxSpec = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("JSON deserialize failed for {}: {}", spec.name, e));
        assert_eq!(
            spec, &deserialized,
            "JSON round-trip data loss for {}",
            spec.name
        );
    }
}

// ---------------------------------------------------------------------------
// Step 7: Verify codegen output is deterministic (same TOML → same output)
// ---------------------------------------------------------------------------

#[test]
fn test_codegen_deterministic() {
    let ws = workspace_root();
    let tomls = find_blox_tomls(&ws);

    for toml_path in &tomls {
        let files1 = generate_from_toml(toml_path)
            .unwrap_or_else(|e| panic!("first codegen failed for {}: {}", toml_path.display(), e));
        let files2 = generate_from_toml(toml_path)
            .unwrap_or_else(|e| panic!("second codegen failed for {}: {}", toml_path.display(), e));

        assert_eq!(
            files1.len(),
            files2.len(),
            "codegen file count not deterministic for {}",
            toml_path.display()
        );

        for (a, b) in files1.iter().zip(files2.iter()) {
            assert_eq!(
                a.0,
                b.0,
                "filename mismatch on re-run for {}",
                toml_path.display()
            );
            assert_eq!(
                a.1,
                b.1,
                "content mismatch on re-run for {}",
                toml_path.display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers (mirrors the viz-export internal logic for comparison)
// ---------------------------------------------------------------------------

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

fn parse_target(s: &str) -> bloxide_viz_export::model::Target {
    let s = s.trim();
    match s {
        "stay" => bloxide_viz_export::model::Target::Stay,
        "reset" | "Reset" => bloxide_viz_export::model::Target::Reset,
        "fail" => bloxide_viz_export::model::Target::Transition("__fail__".to_string()),
        _ => bloxide_viz_export::model::Target::Transition(s.to_string()),
    }
}
