// Copyright 2025 Bloxide, all rights reserved

//! blox.toml → BloxSpec mapping.

use crate::hierarchy::{compute_hierarchy, fill_dropped_handlers, fill_inherited_handlers};
use crate::model::{self, BloxSpec};
use bloxide_codegen::schema::{
    BloxConfig, ContextConfig, StateConfig, TopologyConfig, TransitionConfig,
};
use std::collections::HashMap;

/// Convert a parsed `BloxConfig` into the viz-export `BloxSpec` model.
pub(crate) fn config_to_spec(name: &str, crate_path: &str, config: &BloxConfig) -> BloxSpec {
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
                initial: state_cfg.initial.unwrap_or(false),
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

/// Split a transition event pattern into (message set, variant).
///
/// This DEFINES the `Set::Variant` key used for `Handler::event` and
/// `Event::full_name`; `cargo blox verify` calls this same function to
/// rebuild the key it compares against exported specs, so the two sides of
/// the round-trip can never diverge.
///
/// - Payload is stripped at the first `(`; nested payloads (and any `::`
///   inside them) drop away with it, e.g.
///   `SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))`
///   → `("SupervisorEvent", "Child")`.
/// - Only the first alternative of an or-pattern is used — the handler is
///   recorded under the first alternative, e.g.
///   `PeerCtrl::AddPeer(_) | PeerCtrl::RemovePeer(_)` → `("PeerCtrl", "AddPeer")`.
/// - Type and variant split at the LAST `::`, so qualified paths keep their
///   leading segments on the message set: `foo::CounterMsg::Tick(_)` →
///   `("foo::CounterMsg", "Tick")`.
/// - A pattern with no `::` (wildcard `_`, bare variant) yields
///   `("Unknown", <pattern>)`.
pub fn parse_event_pattern(pattern: &str) -> (String, String) {
    let pattern = pattern.trim();
    // Cut at the first `(` (payload start) or `|` (or-pattern separator). A
    // `|` inside parens is always preceded by its `(`, so the first of the
    // two characters also ends the first top-level alternative.
    let end = pattern.find(['(', '|']).unwrap_or(pattern.len());
    let path = pattern[..end].trim();
    if let Some(pos) = path.rfind("::") {
        let message_set = path[..pos].trim().to_string();
        let variant = path[pos + 2..].trim().to_string();
        (message_set, variant)
    } else {
        ("Unknown".to_string(), path.to_string())
    }
}
