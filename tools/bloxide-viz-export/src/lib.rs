// Copyright 2025 Bloxide, all rights reserved
pub mod model;

mod discover;
mod extract;
mod hierarchy;

use bloxide_codegen::schema::BloxConfig;
use std::fs;
use std::path::{Path, PathBuf};

pub use discover::find_blox_tomls;
pub use extract::parse_event_pattern;
pub use model::BloxSpec;

/// Scan a workspace for `blox.toml` files and export them as `BloxSpec` structs.
///
/// Returns one `BloxSpec` per discovered blox crate.
pub fn export_workspace(workspace_path: &Path) -> Result<Vec<BloxSpec>, String> {
    let blox_tomls = find_blox_tomls(workspace_path);

    if blox_tomls.is_empty() {
        return Err("No blox.toml files found.".to_string());
    }

    let mut specs = Vec::new();

    for (name, toml_path) in &blox_tomls {
        let content = fs::read_to_string(toml_path)
            .map_err(|e| format!("could not read {}: {}", toml_path.display(), e))?;

        let config: BloxConfig = toml::from_str(&content)
            .map_err(|e| format!("failed to parse {}: {}", toml_path.display(), e))?;

        let crate_path = toml_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let spec = extract::config_to_spec(name, &crate_path, &config);
        specs.push(spec);
    }

    // System specs: one per system.toml in the workspace, carrying the
    // wiring graph (actors, connections, supervisors) used by the System
    // and Supervision views (#123, #128).
    for system_path in find_system_tomls(workspace_path) {
        match export_system_spec(&system_path) {
            Ok(spec) => specs.push(spec),
            Err(e) => eprintln!(
                "bloxide-viz-export: warning: skipping {}: {}",
                system_path.display(),
                e
            ),
        }
    }

    Ok(specs)
}

/// Discover `system.toml` manifests under the workspace (excluding target/).
fn find_system_tomls(workspace_path: &Path) -> Vec<PathBuf> {
    discover::walkdir_tomls(workspace_path)
        .into_iter()
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("system.toml")))
        .filter(|p| {
            !p.components()
                .any(|c| c.as_os_str() == std::ffi::OsStr::new("target"))
        })
        .collect()
}

/// Build a system spec from a `system.toml` manifest.
fn export_system_spec(system_path: &Path) -> Result<BloxSpec, String> {
    let content = fs::read_to_string(system_path)
        .map_err(|e| format!("could not read {}: {}", system_path.display(), e))?;
    let config: bloxide_codegen::schema::SystemConfig = toml::from_str(&content)
        .map_err(|e| format!("failed to parse {}: {}", system_path.display(), e))?;

    let app_name = config.system.name.clone().unwrap_or_else(|| {
        system_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("system")
            .to_string()
    });

    let actors = config
        .actors
        .iter()
        .map(|a| model::WiringActor {
            blox: a.blox.clone(),
            name: a.name.clone(),
        })
        .collect();

    let mut connections = Vec::new();
    for actor in &config.actors {
        for (field, source) in &actor.inject {
            if source.source == "actor" {
                if let Some(from) = &source.actor {
                    connections.push(model::WiringConnection {
                        from: from.clone(),
                        to: actor.name.clone(),
                        message: field.clone(),
                        channel_capacity: actor.channel_capacity,
                    });
                }
            }
        }
    }

    let supervisors = config
        .supervision
        .iter()
        .map(|sup| model::WiringSupervisor {
            name: sup.supervisor.clone(),
            strategy: sup.strategy.clone(),
            children: sup
                .children
                .iter()
                .map(|c| {
                    let policy = sup.policies.get(c);
                    model::WiringSupervisorChild {
                        actor: c.clone(),
                        restart_max: policy.and_then(|p| p.restart.as_ref().map(|r| r.max)),
                        stop: policy.and_then(|p| p.stop),
                    }
                })
                .collect(),
        })
        .collect();

    Ok(BloxSpec {
        name: app_name,
        crate_path: system_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        states: Vec::new(),
        events: Vec::new(),
        handlers: Vec::new(),
        entry_exit: std::collections::HashMap::new(),
        message_sets: Vec::new(),
        messages: Vec::new(),
        actions: Vec::new(),
        context: None,
        wiring: Some(model::WiringGraph {
            runtime: config.system.runtime.clone(),
            actors,
            connections,
            supervisors,
        }),
    })
}

/// Write exported specs as JSON files to the given output directory.
pub fn write_specs_to_json(specs: &[BloxSpec], output_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    for spec in specs {
        let output_path = output_dir.join(format!("{}.json", spec.name.to_lowercase()));
        let json = serde_json::to_string_pretty(spec)
            .map_err(|e| format!("Failed to serialize JSON: {}", e))?;
        fs::write(&output_path, json)
            .map_err(|e| format!("Failed to write {}: {}", output_path.display(), e))?;
    }

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn export_finds_blox_tomls() {
        let ws = workspace_root();
        let tomls = find_blox_tomls(&ws);
        assert!(
            tomls.len() >= 5,
            "expected at least 5 blox.toml files, found {}: {:?}",
            tomls.len(),
            tomls.iter().map(|(n, _)| n).collect::<Vec<_>>()
        );
    }

    #[test]
    fn export_workspace_returns_specs_with_content() {
        let ws = workspace_root();
        let specs = export_workspace(&ws).expect("export should succeed");
        assert!(
            specs.len() >= 5,
            "expected at least 5 specs, got {}",
            specs.len()
        );

        // Verify each spec has content: actor/topology crates have states,
        // message-definition crates have messages (#124), system specs have
        // a wiring graph (#123, #128).
        for spec in &specs {
            assert!(
                !spec.states.is_empty() || !spec.messages.is_empty() || spec.wiring.is_some(),
                "spec '{}' has neither states, messages, nor a wiring graph",
                spec.name
            );
        }
    }

    #[test]
    fn ping_spec_contents() {
        let ws = workspace_root();
        let specs = export_workspace(&ws).expect("export should succeed");
        let ping = specs
            .iter()
            .find(|s| s.name == "Ping")
            .expect("Ping spec should exist");

        // States
        assert!(ping.states.iter().any(|s| s.name == "Operating"));
        assert!(ping.states.iter().any(|s| s.name == "Active"));
        assert!(ping.states.iter().any(|s| s.name == "Paused"));
        assert!(ping.states.iter().any(|s| s.name == "Error"));

        // Operating is composite
        let operating = ping.states.iter().find(|s| s.name == "Operating").unwrap();
        assert_eq!(operating.kind, model::StateKind::Composite);

        // Error is error
        let error = ping.states.iter().find(|s| s.name == "Error").unwrap();
        assert_eq!(error.kind, model::StateKind::Error);

        // Events
        assert!(ping
            .events
            .iter()
            .any(|e| e.full_name == "PingPongMsg::Pong"));
        assert!(ping
            .events
            .iter()
            .any(|e| e.full_name == "PingPongMsg::Resume"));

        // Handlers — Active has a handler for Pong
        assert!(ping
            .handlers
            .iter()
            .any(|h| h.state == "Active" && h.event == "PingPongMsg::Pong"));

        // Active entry actions
        let entry = ping.entry_exit.get("Active").expect("Active has entry");
        assert!(!entry.on_entry.is_empty());

        // Paused entry/exit
        let paused_entry = ping
            .entry_exit
            .get("Paused")
            .expect("Paused has entry/exit");
        assert!(paused_entry
            .on_entry
            .iter()
            .any(|a| a.contains("schedule_pause_timer") || a.contains("stub")));
        assert!(paused_entry
            .on_exit
            .iter()
            .any(|a| a.contains("cancel_pause_timer") || a.contains("stub")));

        // Context — no more B generic, fields are inline
        let ctx = ping.context.as_ref().expect("Ping has context");
        assert_eq!(ctx.struct_name, "PingCtx");
        assert!(ctx.uses.iter().any(|f| f.name == "peer_ref"));
        // State fields from [[context.fields]] (round, current_timer) are
        // exported alongside the auto-emitted self_id and uses fields.
        assert!(ctx.fields.iter().any(|f| f.name == "self_id"));
        assert!(ctx.fields.iter().any(|f| f.name == "round"));
        assert!(ctx.fields.iter().any(|f| f.name == "current_timer"));
    }

    #[test]
    fn counter_spec_contents() {
        let ws = workspace_root();
        let specs = export_workspace(&ws).expect("export should succeed");
        let counter = specs
            .iter()
            .find(|s| s.name == "Counter")
            .expect("Counter spec should exist");

        // States
        assert!(counter.states.iter().any(|s| s.name == "Ready"));

        // Ready handler for Tick
        assert!(counter
            .handlers
            .iter()
            .any(|h| h.state == "Ready" && h.event == "CounterMsg::Tick"));

        // The Ready handler should have guard branches
        let ready_handler = counter
            .handlers
            .iter()
            .find(|h| h.state == "Ready" && h.event == "CounterMsg::Tick")
            .unwrap();
        assert!(
            !ready_handler.guard.branches.is_empty(),
            "Ready handler should have guard branches"
        );
    }

    #[test]
    fn pool_spec_contents() {
        let ws = workspace_root();
        let specs = export_workspace(&ws).expect("export should succeed");
        let pool = specs
            .iter()
            .find(|s| s.name == "Pool")
            .expect("Pool spec should exist");

        // States
        assert!(pool.states.iter().any(|s| s.name == "Idle"));
        assert!(pool.states.iter().any(|s| s.name == "Spawning"));
        assert!(pool.states.iter().any(|s| s.name == "Active"));

        // Idle → Spawning on SpawnWorker
        assert!(pool.handlers.iter().any(|h| h.state == "Idle"
            && h.event == "PoolMsg::SpawnWorker"
            && h.target.display() == "Spawning"));

        // Spawning → Active on SpawnReply
        assert!(pool.handlers.iter().any(|h| h.state == "Spawning"
            && h.event == "PoolEvent::SpawnReply"
            && h.target.display() == "Active"));
    }

    #[test]
    fn json_serialization() {
        let ws = workspace_root();
        let specs = export_workspace(&ws).expect("export should succeed");

        // Every spec should serialize to JSON without error
        for spec in &specs {
            let json = serde_json::to_string_pretty(spec);
            assert!(json.is_ok(), "failed to serialize spec '{}'", spec.name);

            // And deserialize back
            let json_str = json.unwrap();
            let back: Result<BloxSpec, _> = serde_json::from_str(&json_str);
            assert!(back.is_ok(), "failed to deserialize spec '{}'", spec.name);
        }
    }

    #[test]
    fn writes_spec_json_files() {
        let ws = workspace_root();
        let specs = export_workspace(&ws).expect("export should succeed");
        let tmp = std::env::temp_dir().join("bloxide-viz-export-test");
        // Clean up any previous run
        let _ = fs::remove_dir_all(&tmp);

        write_specs_to_json(&specs, &tmp).expect("write should succeed");

        // Check that at least one JSON file was written
        let files: Vec<_> = fs::read_dir(&tmp)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("json")))
            .collect();
        assert!(
            files.len() >= 5,
            "expected at least 5 JSON files, found {}",
            files.len()
        );

        // Clean up
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn inherited_handlers() {
        let ws = workspace_root();
        let specs = export_workspace(&ws).expect("export should succeed");
        let ping = specs
            .iter()
            .find(|s| s.name == "Ping")
            .expect("Ping spec should exist");

        // Paused has no explicit Pong handler, so it should inherit from
        // its parent (Operating) which does handle Pong.
        let paused_inherited = ping.handlers.iter().find(|h| {
            h.state == "Paused"
                && h.event == "PingPongMsg::Pong"
                && matches!(h.source, model::HandlerSource::Inherited(_))
        });
        assert!(
            paused_inherited.is_some(),
            "Paused should inherit Pong handler from Operating"
        );
    }

    #[test]
    fn wildcard_uses_unknown_message_set() {
        assert_eq!(
            parse_event_pattern("_"),
            ("Unknown".to_string(), "_".to_string())
        );
    }

    #[test]
    fn unit_variant_without_payload() {
        assert_eq!(
            parse_event_pattern("CounterMsg::Tick"),
            ("CounterMsg".to_string(), "Tick".to_string())
        );
    }

    #[test]
    fn tuple_payload_is_stripped() {
        assert_eq!(
            parse_event_pattern("PingPongMsg::Ping(ping)"),
            ("PingPongMsg".to_string(), "Ping".to_string())
        );
    }

    #[test]
    fn nested_struct_payload_is_stripped() {
        assert_eq!(
            parse_event_pattern(
                "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))"
            ),
            ("SupervisorEvent".to_string(), "Child".to_string())
        );
    }

    #[test]
    fn or_pattern_keeps_first_alternative() {
        assert_eq!(
            parse_event_pattern("PeerCtrl::AddPeer(_) | PeerCtrl::RemovePeer(_)"),
            ("PeerCtrl".to_string(), "AddPeer".to_string())
        );
    }

    #[test]
    fn or_pattern_with_unit_first_alternative() {
        assert_eq!(
            parse_event_pattern("PeerCtrl::AddPeer | PeerCtrl::RemovePeer(_)"),
            ("PeerCtrl".to_string(), "AddPeer".to_string())
        );
    }

    #[test]
    fn multi_segment_path_splits_at_last_separator() {
        assert_eq!(
            parse_event_pattern("crate::CounterMsg::Tick(_)"),
            ("crate::CounterMsg".to_string(), "Tick".to_string())
        );
    }

    #[test]
    fn multi_segment_unit_variant() {
        assert_eq!(
            parse_event_pattern("bloxide_child_management::ChildCtrl::WatchdogTick"),
            (
                "bloxide_child_management::ChildCtrl".to_string(),
                "WatchdogTick".to_string()
            )
        );
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        assert_eq!(
            parse_event_pattern("  CounterMsg::Tick(_)  "),
            ("CounterMsg".to_string(), "Tick".to_string())
        );
    }
}
