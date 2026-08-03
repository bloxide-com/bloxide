// Copyright 2025 Bloxide, all rights reserved
pub mod model;

use bloxide_codegen::schema::{
    BloxConfig, ContextConfig, StateConfig, TopologyConfig, TransitionConfig,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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

        let spec = config_to_spec(name, &crate_path, &config);
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
    walkdir_tomls(workspace_path)
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

// ---------------------------------------------------------------------------
// blox.toml → BloxSpec mapping
// ---------------------------------------------------------------------------

/// Convert a parsed `BloxConfig` into the viz-export `BloxSpec` model.
fn config_to_spec(name: &str, crate_path: &str, config: &BloxConfig) -> BloxSpec {
    let mut spec = BloxSpec {
        name: name.to_string(),
        crate_path: crate_path.to_string(),
        states: Vec::new(),
        events: Vec::new(),
        handlers: Vec::new(),
        entry_exit: HashMap::new(),
        message_sets: Vec::new(),
        messages: Vec::new(),
        actions: Vec::new(),
        context: None,
        wiring: None,
    };

    // --- States ---
    if let Some(topology) = &config.topology {
        extract_states(&mut spec, topology);
        extract_transitions(&mut spec, topology);
        extract_entry_exit(&mut spec, topology);
    }

    // --- Events from mailboxes ---
    // Mailboxes tell us which message types the actor receives. The actual
    // event variants (e.g. PingPongMsg::Pong) come from transitions. But for
    // crates with no declarative transitions (handler_fns-only), we derive
    // events from the [messages] section if present.
    if spec.events.is_empty() {
        if let Some(messages) = &config.messages {
            for msg_enum in messages {
                for variant in &msg_enum.variants {
                    let full_name = format!("{}::{}", msg_enum.name, variant.name);
                    spec.events.push(model::Event {
                        message_set: msg_enum.name.clone(),
                        variant: variant.name.clone(),
                        full_name,
                    });
                }
            }
        }
    }

    // --- Message definitions ---
    if let Some(messages) = &config.messages {
        let crate_name = crate_path
            .rsplit('/')
            .next()
            .unwrap_or(crate_path)
            .to_string();
        for msg_enum in messages {
            let variants: Vec<model::MessageVariant> = msg_enum
                .variants
                .iter()
                .map(|v| {
                    let fields: Vec<String> = v
                        .fields
                        .iter()
                        .map(|f| format!("{}: {}", f.name, f.ty))
                        .collect();
                    model::MessageVariant {
                        name: v.name.clone(),
                        fields,
                    }
                })
                .collect();
            spec.messages.push(model::MessageDef {
                crate_name: crate_name.clone(),
                enum_name: msg_enum.name.clone(),
                variants,
            });
        }
    }

    // --- Context ---
    if let Some(context) = &config.context {
        extract_context(&mut spec, context);
        extract_actions(&mut spec, context, config.topology.as_ref());
    }

    // --- Post-processing: compute hierarchy, inherited/dropped handlers ---
    compute_hierarchy(&mut spec.states);
    fill_inherited_handlers(&mut spec.handlers, &spec.states);
    fill_dropped_handlers(&mut spec.handlers, &spec.states, &spec.events);

    // Build message sets from events
    let mut sets: HashMap<String, Vec<String>> = HashMap::new();
    for event in &spec.events {
        sets.entry(event.message_set.clone())
            .or_default()
            .push(event.variant.clone());
    }
    spec.message_sets = sets
        .into_iter()
        .map(|(name, variants)| model::MessageSet { name, variants })
        .collect();

    spec
}

fn extract_states(spec: &mut BloxSpec, topology: &TopologyConfig) {
    for state_cfg in &topology.states {
        let kind = state_kind(state_cfg);
        let name = state_cfg.name.clone();
        let parent = state_cfg.parent.clone();

        if !spec.states.iter().any(|s| s.name == name) {
            spec.states.push(model::State {
                name,
                kind,
                parent,
                description: String::new(),
                depth: 0,
            });
        }
    }
}

fn state_kind(state_cfg: &StateConfig) -> model::StateKind {
    if state_cfg.error.unwrap_or(false) {
        model::StateKind::Error
    } else if state_cfg.composite.unwrap_or(false) {
        model::StateKind::Composite
    } else {
        model::StateKind::Leaf
    }
}

fn extract_transitions(spec: &mut BloxSpec, topology: &TopologyConfig) {
    for trans in &topology.transitions {
        let (message_set, variant) = parse_event_pattern(&trans.event);
        let full_event = format!("{}::{}", message_set, variant);

        // Add event if not already present
        if !spec.events.iter().any(|e| e.full_name == full_event) {
            spec.events.push(model::Event {
                message_set: message_set.clone(),
                variant: variant.clone(),
                full_name: full_event.clone(),
            });
        }

        // Determine target and guard branches
        let (target, guard_branches) = build_target_and_guards(trans);

        let guard = model::Guard {
            description: build_guard_description(trans, &guard_branches),
            raw: build_guard_raw(&guard_branches, &target),
            branches: guard_branches,
        };

        let label = build_handler_label(&trans.actions, &target);

        spec.handlers.push(model::Handler {
            state: trans.state.clone(),
            event: full_event,
            label,
            pattern: trans.event.clone(),
            feature: trans.feature.clone(),
            actions: trans.actions.clone(),
            guard,
            target,
            source: model::HandlerSource::Explicit,
            on_entry: Vec::new(),
            on_exit: Vec::new(),
        });
    }
}

/// Render the raw guard as an if/else-if/else chain (the Rust decision text).
fn build_guard_raw(branches: &[model::GuardBranch], fallback: &model::Target) -> String {
    if branches.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut has_wildcard = false;
    for (i, b) in branches.iter().enumerate() {
        if b.condition == "_" {
            has_wildcard = true;
            out.push_str(&format!(" else {{ {} }}", b.target.display()));
        } else if i == 0 {
            out.push_str(&format!("if {} {{ {} }}", b.condition, b.target.display()));
        } else {
            out.push_str(&format!(
                " else if {} {{ {} }}",
                b.condition,
                b.target.display()
            ));
        }
    }
    if !has_wildcard {
        out.push_str(&format!(" else {{ {} }}", fallback.display()));
    }
    out
}

fn build_target_and_guards(trans: &TransitionConfig) -> (model::Target, Vec<model::GuardBranch>) {
    if trans.guards.is_empty() {
        let target = parse_target(&trans.target);
        (target, Vec::new())
    } else {
        // Build guard branches from the guards list
        let branches: Vec<model::GuardBranch> = trans
            .guards
            .iter()
            .map(|g| model::GuardBranch {
                condition: g.condition.clone(),
                target: parse_target(&g.target),
            })
            .collect();
        // The top-level target is the fallback (_ arm)
        let fallback = parse_target(&trans.target);
        (fallback, branches)
    }
}

fn parse_target(s: &str) -> model::Target {
    let s = s.trim();
    match s {
        "stay" => model::Target::Stay,
        "reset" | "Reset" => model::Target::Reset,
        "stop" => model::Target::Stop,
        "done" => model::Target::Done,
        "fail" => model::Target::Fail,
        _ => model::Target::Transition(s.to_string()),
    }
}

fn build_guard_description(trans: &TransitionConfig, branches: &[model::GuardBranch]) -> String {
    if branches.is_empty() {
        parse_target(&trans.target).display()
    } else {
        let lines: Vec<String> = branches
            .iter()
            .map(|b| format!("{} => {}", b.condition, b.target.display()))
            .collect();
        format!(
            "{}\n_ => {}",
            lines.join("\n"),
            parse_target(&trans.target).display()
        )
    }
}

fn build_handler_label(actions: &[String], target: &model::Target) -> String {
    if actions.is_empty() {
        target.display()
    } else {
        let action_label = if actions.len() == 1 {
            actions[0].clone()
        } else {
            format!("{} actions", actions.len())
        };
        format!("{} → {}", action_label, target.display())
    }
}

fn extract_entry_exit(spec: &mut BloxSpec, topology: &TopologyConfig) {
    for entry in &topology.entry {
        let ee = spec
            .entry_exit
            .entry(entry.state.clone())
            .or_insert_with(|| model::EntryExit {
                on_entry: Vec::new(),
                on_exit: Vec::new(),
            });
        ee.on_entry = entry.actions.clone();
    }

    for exit in &topology.exit {
        let ee = spec
            .entry_exit
            .entry(exit.state.clone())
            .or_insert_with(|| model::EntryExit {
                on_entry: Vec::new(),
                on_exit: Vec::new(),
            });
        ee.on_exit = exit.actions.clone();
    }
}

fn extract_context(spec: &mut BloxSpec, context: &ContextConfig) {
    // Auto-emitted fields: self_id (always). No more B generic.

    let mut fields: Vec<model::ContextField> = vec![model::ContextField {
        name: "self_id".to_string(),
        ty: "ActorId".to_string(),
        annotations: Vec::new(),
    }];

    // Context fields from [[context.uses]] with field = "..." are plain struct fields.
    for u in &context.uses {
        if let (Some(name), Some(ty)) = (&u.field, &u.field_type) {
            {
                fields.push(model::ContextField {
                    name: name.clone(),
                    ty: ty.clone(),
                    annotations: Vec::new(),
                });
            }
        }
    }

    // Fields declared directly in [[context.fields]] (state fields moved from B).
    for f in &context.fields {
        fields.push(model::ContextField {
            name: f.name.clone(),
            ty: f.r#type.clone(),
            annotations: Vec::new(),
        });
    }

    // Fields contributed by composable context crates (`[[context.uses]]`).
    let mut uses: Vec<model::ContextField> = Vec::new();
    for u in &context.uses {
        if let (Some(name), Some(ty)) = (&u.field, &u.field_type) {
            uses.push(model::ContextField {
                name: name.clone(),
                ty: ty.clone(),
                annotations: Vec::new(),
            });
        }
        for f in &u.fields {
            uses.push(model::ContextField {
                name: f.name.clone(),
                ty: f.ty.clone(),
                annotations: Vec::new(),
            });
        }
    }

    spec.context = Some(model::ContextDef {
        struct_name: context.name.clone(),
        fields,
        uses,
    });
}

/// Export action declarations as renderable definitions (#124): crate,
/// function name, and a reconstructed signature from use site + fields +
/// event payload.
fn extract_actions(
    spec: &mut BloxSpec,
    context: &ContextConfig,
    topology: Option<&TopologyConfig>,
) {
    // Field name → type map from the context definition.
    let mut field_types: HashMap<String, String> = HashMap::new();
    field_types.insert("self_id".to_string(), "ActorId".to_string());
    for u in &context.uses {
        if let (Some(name), Some(ty)) = (&u.field, &u.field_type) {
            field_types.insert(name.clone(), ty.clone());
        }
        for f in &u.fields {
            field_types.insert(f.name.clone(), f.ty.clone());
        }
    }
    for f in &context.fields {
        field_types.insert(f.name.clone(), f.r#type.clone());
    }

    // Use-site classification: an action whose name appears in a transition
    // action list is annotated `-> ActionResult`; entry/exit-only and unused
    // actions get no return annotation.
    let transition_actions: Vec<&str> = topology
        .map(|t| {
            t.transitions
                .iter()
                .flat_map(|tr| tr.actions.iter())
                .filter_map(|a| a.strip_prefix("Self::"))
                .collect()
        })
        .unwrap_or_default();

    for a in &context.actions {
        let fn_name = a.fn_name.clone().unwrap_or_else(|| a.name.clone());
        let crate_name = a.crate_name.clone().unwrap_or_else(|| "crate".to_string());
        let mut params: Vec<String> = a
            .fields
            .iter()
            .map(|f| {
                let (name, mode) = match f.rsplit_once(':') {
                    Some((n, m)) => (n, m),
                    None => (f.as_str(), ""),
                };
                let ty = field_types
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| "_".to_string());
                match mode {
                    "ref" => format!("{}: &{}", name, ty),
                    "mut" => format!("{}: &mut {}", name, ty),
                    _ => format!("{}: {}", name, ty),
                }
            })
            .collect();
        if let Some(payload) = &a.event_payload {
            params.push(format!("{}: &_", payload));
        } else if a.event_arg {
            params.push("ev: &Event".to_string());
        }
        let ret = if transition_actions.contains(&a.name.as_str()) {
            " -> ActionResult"
        } else {
            ""
        };
        let signature = format!("fn {}({}){}", fn_name, params.join(", "), ret);
        spec.actions.push(model::ActionDef {
            crate_name,
            function_name: fn_name,
            signature,
        });
    }
}

fn parse_event_pattern(pattern: &str) -> (String, String) {
    let pattern = pattern.trim();
    // Remove trailing parenthetical content like (_)
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

// ---------------------------------------------------------------------------
// Hierarchy and handler post-processing
// ---------------------------------------------------------------------------

fn compute_hierarchy(states: &mut [model::State]) {
    for state in states.iter_mut() {
        if state.parent.is_some() {
            state.depth = 1;
        }
    }
}

fn fill_inherited_handlers(handlers: &mut Vec<model::Handler>, states: &[model::State]) {
    let composites: Vec<&model::State> = states
        .iter()
        .filter(|s| matches!(s.kind, model::StateKind::Composite))
        .collect();

    for composite in &composites {
        let composite_handlers: Vec<model::Handler> = handlers
            .iter()
            .filter(|h| h.state == composite.name)
            .cloned()
            .collect();

        let children: Vec<&model::State> = states
            .iter()
            .filter(|s| s.parent.as_ref() == Some(&composite.name))
            .collect();

        for child in children {
            for ch in &composite_handlers {
                if !handlers
                    .iter()
                    .any(|h| h.state == child.name && h.event == ch.event)
                {
                    handlers.push(model::Handler {
                        state: child.name.clone(),
                        event: ch.event.clone(),
                        label: format!("⬇️ {} ({})", ch.label, composite.name),
                        pattern: ch.pattern.clone(),
                        feature: ch.feature.clone(),
                        actions: ch.actions.clone(),
                        guard: ch.guard.clone(),
                        target: ch.target.clone(),
                        source: model::HandlerSource::Inherited(composite.name.clone()),
                        on_entry: Vec::new(),
                        on_exit: Vec::new(),
                    });
                }
            }
        }
    }
}

fn fill_dropped_handlers(
    handlers: &mut Vec<model::Handler>,
    states: &[model::State],
    events: &[model::Event],
) {
    let leaf_states: Vec<&model::State> = states.iter().filter(|s| s.kind.is_leaf()).collect();

    for state in leaf_states {
        for event in events {
            if !handlers
                .iter()
                .any(|h| h.state == state.name && h.event == event.full_name)
            {
                handlers.push(model::Handler {
                    state: state.name.clone(),
                    event: event.full_name.clone(),
                    label: "∅".to_string(),
                    pattern: String::new(),
                    feature: None,
                    actions: Vec::new(),
                    guard: model::Guard {
                        description: "No handler — dropped".to_string(),
                        raw: String::new(),
                        branches: Vec::new(),
                    },
                    target: model::Target::Stay,
                    source: model::HandlerSource::Dropped,
                    on_entry: Vec::new(),
                    on_exit: Vec::new(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

pub fn find_blox_tomls(workspace_path: &Path) -> Vec<(String, PathBuf)> {
    let mut crates = Vec::new();

    for entry in walkdir_tomls(workspace_path) {
        let path = entry;
        if path.file_name() != Some(std::ffi::OsStr::new("blox.toml")) {
            continue;
        }

        let crate_path = path.parent();

        // Skip if this is inside a target/ build directory
        if path
            .components()
            .any(|c| c.as_os_str() == std::ffi::OsStr::new("target"))
        {
            continue;
        }

        if let Some(crate_path) = crate_path {
            if let Ok(content) = fs::read_to_string(&path) {
                // Actor/topology crates and message-definition crates both
                // produce specs (#124 — message definitions are renderable).
                if content.contains("[actor]")
                    || content.contains("[topology]")
                    || content.contains("[[messages]]")
                {
                    let dir_name = crate_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown");

                    // Try to get the actor name from the TOML; fall back to
                    // Pascal-casing the directory name
                    let name = extract_actor_name(&content).unwrap_or_else(|| {
                        let mut chars = dir_name.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(first) => {
                                first.to_uppercase().collect::<String>() + chars.as_str()
                            }
                        }
                    });

                    crates.push((name, path.clone()));
                }
            }
        }
    }

    crates.sort_by(|a, b| a.0.cmp(&b.0));
    crates.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    crates
}

/// Extract the `[actor] name = "..."` value from a blox.toml string without
/// full deserialization (used only for sorting/naming in discovery).
fn extract_actor_name(content: &str) -> Option<String> {
    let config: BloxConfig = toml::from_str(content).ok()?;
    config.actor.map(|a| a.name)
}

/// Walk a directory tree for `blox.toml` files, up to max_depth 6.
fn walkdir_tomls(workspace_path: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    walk_dir_recursive(workspace_path, 0, 6, &mut results);
    results
}

fn walk_dir_recursive(path: &Path, depth: usize, max_depth: usize, results: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();

        // Skip hidden directories and target/
        if let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "target" {
                continue;
            }
        }

        if entry_path.is_dir() {
            walk_dir_recursive(&entry_path, depth + 1, max_depth, results);
        } else if matches!(
            entry_path.file_name().and_then(|n| n.to_str()),
            Some("blox.toml") | Some("system.toml")
        ) {
            results.push(entry_path);
        }
    }
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
    fn test_export_finds_blox_tomls() {
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
    fn test_export_workspace() {
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
    fn test_ping_spec_contents() {
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
        // current_timer and round are now in [[context.fields]], not [[context.uses]]
        // The viz-export parser will be updated in Phase 3 to parse context.fields
        // For now, just verify the struct name and accessor fields are correct
    }

    #[test]
    fn test_counter_spec_contents() {
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
    fn test_pool_spec_contents() {
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
    fn test_json_serialization() {
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
    fn test_write_specs_to_json() {
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
    fn test_inherited_handlers() {
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
}
