// Copyright 2025 Bloxide, all rights reserved
//! System-level codegen: generate concrete `spec_skeleton.rs` with real action closures.
//!
//! The blox-level codegen generates stub closures for `Self::` prefixed actions.
//! This module replaces those stubs with concrete closures that call the real
//! functions from context crates or impl crates, based on `[[context.actions]]`
//! declarations in the blox.toml.
//!
//! The topology (states, transitions, guards) is identical to the blox-level
//! spec — only the action closures change. Stubs become real function calls.

use std::collections::HashMap;

use crate::schema::{BloxConfig, ContextActionConfig};
use crate::spec_skeleton;

/// Extract the action name from a `Self::name` string.
fn strip_self_prefix(action: &str) -> Option<&str> {
    action.strip_prefix("Self::")
}

/// Convert a PascalCase string to snake_case (e.g. "SpawnReply" → "spawn_reply").
fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

/// Parse an event pattern like `"PingPongMsg::Ping(_)"` and replace the
/// inner binding with the payload variable name, producing e.g.
/// `"PingPongMsg::Ping(ping)"`. If the pattern has multiple variants
/// (e.g. `"Msg::A(_) | Msg::B(_)"`), the first variant is used.
fn make_payload_pattern(event_pattern: &str, payload_var: &str) -> String {
    // Find the variant path before the `(` and replace the contents with the payload var.
    // Pattern examples:
    //   "PingPongMsg::Ping(_)"          → "PingPongMsg::Ping(ping)"
    //   "WorkerMsg::DoWork(do_work)"    → "WorkerMsg::DoWork(do_work)" (already named)
    //   "PingPongMsg::Ping(_) | ..."     → "PingPongMsg::Ping(ping)" (first variant only)
    let pattern = event_pattern
        .split('|')
        .next()
        .unwrap_or(event_pattern)
        .trim();
    // Find the opening paren
    if let Some(open) = pattern.find('(') {
        let path = &pattern[..open];
        format!("{path}({payload_var})")
    } else {
        // No parens — shouldn't happen for payload actions, but handle gracefully
        format!("{event_pattern}")
    }
}

/// Parse a field specification like `"round:mut"`, `"peer_ref:ref"`, or `"self_id"`.
/// Returns (field_name, access_mode) where access_mode is "mut", "ref", or "" (copy).
fn parse_field_spec(spec: &str) -> (&str, &str) {
    if let Some((name, mode)) = spec.split_once(':') {
        (name, mode)
    } else {
        (spec, "")
    }
}

/// Generate the field access expression for a ctx field.
///
/// - `"field:mut"` → `&mut ctx.field`
/// - `"field:ref"` → `&ctx.field`
/// - `"field"`     → `ctx.field` (copy/owned)
fn field_access(field_spec: &str) -> String {
    let (name, mode) = parse_field_spec(field_spec);
    match mode {
        "mut" => format!("&mut ctx.{name}"),
        "ref" => format!("&ctx.{name}"),
        _ => format!("ctx.{name}"),
    }
}

/// Check whether an action config represents a no-op (empty fields, no payload).
fn is_noop_action(config: &ContextActionConfig) -> bool {
    config.fields.is_empty() && config.event_payload.is_none()
}

/// Generate a concrete action closure for a `Self::` prefixed action.
///
/// The closure calls the real function from the context crate or impl crate,
/// based on the `[[context.actions]]` declaration.
///
/// # Arguments
/// * `action` - The action string from the topology (e.g. `"Self::increment_round"`)
/// * `actions` - The `[[context.actions]]` entries from the blox.toml
/// * `impl_crate` - The impl crate name (for `impl_required = true` actions)
/// * `is_transition` - Whether this is a transition action (vs entry/exit)
///
/// # Returns
/// A token stream for the concrete closure, or `None` if the action is not
/// a `Self::` prefixed action (in which case the caller should use the default
/// `resolve_action` from `spec_skeleton.rs`).
pub fn resolve_concrete_action(
    action: &str,
    actions: &[ContextActionConfig],
    impl_crate: Option<&str>,
    event_type_str: &str,
    event_pattern: Option<&str>,
    is_transition: bool,
) -> Option<proc_macro2::TokenStream> {
    let name = strip_self_prefix(action)?;

    // Find the action declaration by name.
    let config = actions.iter().find(|a| a.name == name)?;

    // No-op actions: empty fields and no event_payload.
    // Generate minimal no-op closures.
    if is_noop_action(config) {
        if is_transition {
            return Some(quote::quote! {
                |_ctx, _ev| { ::bloxide_core::transition::ActionResult::Ok }
            });
        } else {
            return Some(quote::quote! {
                |_ctx| {}
            });
        }
    }

    // Determine the function path.
    // When `crate = "crate"` is set on the action config, use the relative
    // `crate` path (for blox-level codegen where the actions live in the
    // blox crate itself). Otherwise, build an absolute `::crate_name` path.
    let fn_path = if config
        .crate_name
        .as_deref()
        .map(|c| c == "crate")
        .unwrap_or(false)
    {
        "crate".to_string()
    } else if config.impl_required {
        // Use the impl crate from the system.toml.
        let crate_name = impl_crate.unwrap_or("unknown_impl");
        format!("::{}", crate_name.replace('-', "_"))
    } else {
        // Use the crate from the [[context.actions]] entry.
        // The crate_name may already be an absolute path (e.g. "::bloxide_supervisor")
        // if it was translated by generate_concrete_spec_skeleton. In that case,
        // don't prepend another "::".
        let crate_name = config.crate_name.as_deref().unwrap_or("unknown_crate");
        if crate_name.starts_with("::") {
            crate_name.replace('-', "_")
        } else {
            format!("::{}", crate_name.replace('-', "_"))
        }
    };
    let fn_name = config
        .fn_name
        .as_deref()
        .unwrap_or(&config.name)
        .replace('-', "_");
    // Optional module segment between crate path and function name.
    let fn_full_path = match &config.module {
        Some(module) => format!("{fn_path}::{module}::{fn_name}"),
        None => format!("{fn_path}::{fn_name}"),
    };

    // Build the field arguments.
    let field_args: Vec<String> = config.fields.iter().map(|f| field_access(f)).collect();

    // Generate the closure based on kind.
    let has_payload = config.event_payload.is_some();

    if !is_transition {
        // Entry/exit action: |ctx| { <fn_full_path>(<args>); }
        let args = field_args.join(", ");
        let code = format!("|ctx| {{ {fn_full_path}({args}); }}");
        Some(
            syn::parse_str::<proc_macro2::TokenStream>(&code).unwrap_or_else(|_| {
                quote::quote! { |_ctx| { /* error: cannot parse concrete action #name */ } }
            }),
        )
    } else if !has_payload {
        // Transition without event payload.
        // When event_arg is true, pass the full event reference as the last arg:
        //   |ctx, ev| { <fn_full_path>(<args>, ev); ActionResult::Ok }
        // Otherwise (no event needed):
        //   |ctx, _ev| { <fn_full_path>(<args>); ActionResult::Ok }
        let args = if config.event_arg {
            if field_args.is_empty() {
                "ev".to_string()
            } else {
                format!("{}, ev", field_args.join(", "))
            }
        } else {
            field_args.join(", ")
        };
        let ev_param = if config.event_arg { "ev" } else { "_ev" };
        let code = format!(
            "|ctx, {ev_param}| {{ {fn_full_path}({args}); \
             ::bloxide_core::transition::ActionResult::Ok }}"
        );
        Some(syn::parse_str::<proc_macro2::TokenStream>(&code).unwrap_or_else(|_| {
            quote::quote! { |_ctx, _ev| { /* error: cannot parse concrete action #name */ ::bloxide_core::transition::ActionResult::Ok } }
        }))
    } else {
        // Transition with event payload.
        //
        // Two cases:
        //   1. Message-variant pattern (e.g. "PingPongMsg::Ping(_)"):
        //      Uses ev.msg_payload() which returns Option<&MsgEnum>.
        //      → if let Some(PingPongMsg::Ping(ping)) = ev.msg_payload()
        //
        //   2. Event-variant pattern (e.g. "PoolEvent::SpawnReply(_)"):
        //      Uses the per-mailbox accessor (e.g. ev.spawn_reply_payload())
        //      which returns Option<&InnerType>.
        //      → if let Some(spawned_worker) = ev.spawn_reply_payload()
        //
        // The distinction: if the pattern path starts with the event type name
        // (e.g. "PoolEvent"), it's an event-variant pattern. Otherwise it's a
        // message-variant pattern.
        let payload_var = config.event_payload.as_ref().unwrap().replace('-', "_");
        let args_with_payload = if field_args.is_empty() {
            payload_var.clone()
        } else {
            format!("{}, {}", field_args.join(", "), payload_var)
        };

        // Determine which payload accessor to use and the if-let pattern.
        // `event_type_str` is the event enum name (e.g. "PoolEvent<R>").
        let event_type_name = event_type_str
            .split(|c| c == '<' || c == ',')
            .next()
            .unwrap_or("")
            .trim();

        let (payload_accessor, if_let_pattern) = match event_pattern {
            Some(ep) => {
                let pattern = ep.split('|').next().unwrap_or(ep).trim();
                let path = pattern.split('(').next().unwrap_or(pattern).trim();
                let variant_name = path.split("::").last().unwrap_or(path);
                // Extract the first identifier for shorthand classification
                // (e.g. "PeerCtrl" → CtrlShorthand, "WorkerMsg" → MsgShorthand).
                let first_ident: String = path
                    .chars()
                    .skip_while(|c| !c.is_alphabetic() && *c != '_')
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();

                if path.starts_with(event_type_name) {
                    // Event-variant pattern: use the per-mailbox payload accessor.
                    // e.g. "PoolEvent::SpawnReply" → accessor "spawn_reply_payload"
                    let snake = to_snake_case(variant_name);
                    let accessor = format!("ev.{snake}_payload()");
                    // The accessor returns Option<&InnerType>, so bind directly.
                    (accessor, format!("Some({payload_var})"))
                } else if first_ident.ends_with("Ctrl") {
                    // Ctrl shorthand: the Ctrl mailbox wraps a PeerCtrl<M, R>.
                    // Use ctrl_payload() which returns Option<&PeerCtrl<M, R>>.
                    // Bind the whole PeerCtrl value — the action function
                    // (e.g. apply_peer_control) takes &PeerCtrl<M, R>, not a
                    // destructured variant.
                    (format!("ev.ctrl_payload()"), format!("Some({payload_var})"))
                } else {
                    // Message-variant pattern: use msg_payload() and pattern match.
                    let pat = make_payload_pattern(ep, &payload_var);
                    (format!("ev.msg_payload()"), format!("Some({pat})"))
                }
            }
            None => (format!("ev.msg_payload()"), format!("Some({payload_var})")),
        };

        let code = format!(
            "|ctx, ev| {{ \
             if let {if_let_pattern} = {payload_accessor} {{ \
             {fn_full_path}({args_with_payload}); \
             }} \
             ::bloxide_core::transition::ActionResult::Ok \
             }}"
        );
        Some(syn::parse_str::<proc_macro2::TokenStream>(&code).unwrap_or_else(|_| {
            quote::quote! { |_ctx, _ev| { /* error: cannot parse concrete action #name */ ::bloxide_core::transition::ActionResult::Ok } }
        }))
    }
}

/// Build a lookup map of action name → ContextActionConfig.
pub fn build_action_map(blox_config: &BloxConfig) -> HashMap<String, &ContextActionConfig> {
    let mut map = HashMap::new();
    if let Some(context) = &blox_config.context {
        for action in &context.actions {
            map.insert(action.name.clone(), action);
        }
    }
    map
}

/// Generate a complete concrete `spec_skeleton.rs` file for a blox, replacing
/// stub action closures with real function calls based on `[[context.actions]]`.
///
/// This function reuses the topology generation from `spec_skeleton::generate`
/// but replaces the stub `resolve_action` with `resolve_concrete_action` for
/// `Self::` prefixed actions. Non-`Self::` actions (real function path references
/// imported via `spec_imports`) are handled by the default `resolve_action`.
///
/// # Arguments
/// * `blox_config` - The parsed blox.toml configuration
/// * `impl_crate` - The impl crate name from system.toml (for `impl_required = true` actions)
/// * `crate_name` - The blox crate name (for file naming / handler table generation)
/// * `blox_crate_path` - The Rust path prefix for referencing the blox crate's types.
///   Use `"crate"` for blox-level generation, or `"::crate_name"` for system-level.
///
/// # Returns
/// A complete `spec_skeleton.rs` file as a String, with concrete action closures.
pub fn generate_concrete_spec_skeleton(
    blox_config: &BloxConfig,
    impl_crate: Option<&str>,
    crate_name: &str,
    blox_crate_path: &str,
    active_feature: Option<&str>,
) -> anyhow::Result<String> {
    let actor = blox_config
        .actor
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("blox config has no [actor] section"))?;
    let topology = blox_config
        .topology
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("blox config has no [topology] section"))?;
    let context = blox_config
        .context
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("blox config has no [context] section"))?;

    // Collect the action declarations — these are moved into the closure.
    // Translate `crate = "crate"` to the actual blox crate path so that
    // system-level codegen references the blox crate correctly.
    let actions: Vec<ContextActionConfig> = context
        .actions
        .iter()
        .map(|a| {
            let mut a = a.clone();
            if a.crate_name.as_deref() == Some("crate") && blox_crate_path != "crate" {
                // System-level: replace "crate" with the absolute blox crate path.
                a.crate_name = Some(blox_crate_path.to_string());
            }
            a
        })
        .collect();
    let impl_crate_owned = impl_crate.map(|s| s.to_string());

    // Build the concrete action resolver closure.
    // For Self:: prefixed actions, use resolve_concrete_action.
    // For non-Self:: actions (function path references), fall back to the
    // default resolve_action from spec_skeleton.
    let resolver = move |action: &str,
                         ctx_type_str: &str,
                         event_type_str: &str,
                         type_params: &[String],
                         event_pattern: Option<&str>,
                         is_transition: bool|
          -> proc_macro2::TokenStream {
        if let Some(concrete) = resolve_concrete_action(
            action,
            &actions,
            impl_crate_owned.as_deref(),
            event_type_str,
            event_pattern,
            is_transition,
        ) {
            concrete
        } else {
            // Fall back to the default stub/path resolver for non-Self:: actions.
            spec_skeleton::resolve_action(
                action,
                ctx_type_str,
                event_type_str,
                type_params,
                event_pattern,
                is_transition,
            )
        }
    };

    // Use the provided blox_crate_path for referencing types.
    // For blox-level generation this is "crate"; for system-level it's
    // e.g. "::ping_blox".
    spec_skeleton::generate(
        actor,
        topology,
        context,
        blox_config.event.as_ref(),
        crate_name,
        blox_crate_path,
        &resolver,
        active_feature,
    )
}

/// Generate concrete spec_skeleton.rs files for all actors in a system.
///
/// For each actor in the system.toml, this looks up the actor's BloxConfig,
/// and calls `generate_concrete_spec_skeleton` to produce a complete
/// `spec_skeleton.rs` with real action closures replacing stubs.
///
/// Returns a list of (actor_name, spec_code) pairs. The caller writes these
/// to the app's `src/generated/` directory.
pub fn generate_concrete_spec_files(
    blox_configs: &std::collections::BTreeMap<String, BloxConfig>,
    actors: &[crate::schema::ActorInstance],
) -> anyhow::Result<Vec<(String, String)>> {
    let mut results = Vec::new();

    for actor in actors {
        // Skip timer service actors (they have no blox.toml).
        if actor.kind.as_deref() == Some("timer") {
            continue;
        }

        // Look up the blox config by crate name (e.g. "ping-blox").
        let blox_config = blox_configs.get(&actor.blox).ok_or_else(|| {
            anyhow::anyhow!(
                "blox crate '{}' not found for actor '{}'",
                actor.blox,
                actor.name
            )
        })?;

        let blox_crate_path = format!("::{}", actor.blox.replace('-', "_"));
        // Pass the active feature (if any) so the spec skeleton emits only
        // the matching variant without #[cfg] gates.
        let active_feature = actor.features.first().map(|s| s.as_str());
        let code = generate_concrete_spec_skeleton(
            blox_config,
            actor.impl_crate.as_deref(),
            &actor.blox,
            &blox_crate_path,
            active_feature,
        )?;

        results.push((actor.name.clone(), code));
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_action(
        name: &str,
        crate_name: Option<&str>,
        kind: &str,
        fields: Vec<&str>,
        event_payload: Option<&str>,
        impl_required: bool,
    ) -> ContextActionConfig {
        ContextActionConfig {
            name: name.to_string(),
            crate_name: crate_name.map(|s| s.to_string()),
            kind: kind.to_string(),
            fields: fields.iter().map(|s| s.to_string()).collect(),
            event_payload: event_payload.map(|s| s.to_string()),
            event_arg: false,
            impl_required,
            feature: None,
            fn_name: None,
            module: None,
        }
    }

    #[test]
    fn test_resolve_concrete_entry_action() {
        let actions = vec![make_action(
            "s_entry",
            Some("bloxide_core"),
            "entry",
            vec![],
            None,
            false,
        )];
        let result =
            resolve_concrete_action("Self::s_entry", &actions, None, "TestEvent", None, false);
        assert!(result.is_some());
        let tokens = result.unwrap().to_string();
        // No-op entry action (empty fields, no payload)
        assert!(tokens.contains("_ctx"));
    }

    #[test]
    fn test_resolve_concrete_transition_no_payload() {
        let actions = vec![make_action(
            "increment_round",
            Some("blox_ctx_rounds"),
            "transition",
            vec!["round:mut"],
            None,
            false,
        )];
        let result = resolve_concrete_action(
            "Self::increment_round",
            &actions,
            None,
            "TestEvent",
            None,
            true,
        );
        assert!(result.is_some());
        let tokens = result.unwrap().to_string();
        eprintln!("DEBUG tokens: {tokens}");
        assert!(tokens.contains("blox_ctx_rounds"));
        assert!(tokens.contains("increment_round"));
        assert!(tokens.contains("ctx . round"));
        assert!(tokens.contains("ActionResult"));
    }

    #[test]
    fn test_resolve_concrete_transition_with_payload() {
        let actions = vec![make_action(
            "process_work",
            None,
            "transition",
            vec!["task_id:mut", "result:mut"],
            Some("do_work"),
            true,
        )];
        let result = resolve_concrete_action(
            "Self::process_work",
            &actions,
            Some("tokio_pool_demo_impl"),
            "WorkerEvent",
            Some("WorkerMsg::DoWork(_)"),
            true,
        );
        assert!(result.is_some());
        let tokens = result.unwrap().to_string();
        assert!(tokens.contains("tokio_pool_demo_impl"));
        assert!(tokens.contains("process_work"));
        assert!(tokens.contains("do_work"));
        assert!(tokens.contains("msg_payload"));
        assert!(tokens.contains("ActionResult"));
    }

    #[test]
    fn test_resolve_concrete_non_self_action() {
        let actions = vec![];
        let result =
            resolve_concrete_action("some_function", &actions, None, "TestEvent", None, true);
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_concrete_impl_required_no_impl_crate() {
        let actions = vec![make_action(
            "process_work",
            None,
            "transition",
            vec!["task_id:mut"],
            Some("do_work"),
            true,
        )];
        // Without an impl_crate, it should still generate something (with "unknown_impl")
        let result = resolve_concrete_action(
            "Self::process_work",
            &actions,
            None,
            "TestEvent",
            None,
            true,
        );
        assert!(result.is_some());
        let tokens = result.unwrap().to_string();
        assert!(tokens.contains("unknown_impl"));
    }

    #[test]
    fn test_field_access_modes() {
        assert_eq!(field_access("round:mut"), "&mut ctx.round");
        assert_eq!(field_access("peer_ref:ref"), "&ctx.peer_ref");
        assert_eq!(field_access("self_id"), "ctx.self_id");
    }

    #[test]
    fn test_strip_self_prefix() {
        assert_eq!(
            strip_self_prefix("Self::increment_round"),
            Some("increment_round")
        );
        assert_eq!(strip_self_prefix("some_function"), None);
    }

    #[test]
    fn test_noop_entry_action() {
        // bhsm-tst style: empty fields, no event_payload
        let actions = vec![make_action(
            "s_entry",
            Some("bloxide_core"),
            "entry",
            vec![],
            None,
            false,
        )];
        let result =
            resolve_concrete_action("Self::s_entry", &actions, None, "TestEvent", None, false);
        assert!(result.is_some());
        let tokens = result.unwrap().to_string();
        // Should be a no-op: |_ctx| {}
        assert!(tokens.contains("_ctx"));
        // Should NOT contain a function call
        assert!(!tokens.contains("s_entry"));
    }

    #[test]
    fn test_noop_transition_action() {
        let actions = vec![make_action(
            "s_i",
            Some("bloxide_core"),
            "transition",
            vec![],
            None,
            false,
        )];
        let result = resolve_concrete_action("Self::s_i", &actions, None, "TestEvent", None, true);
        assert!(result.is_some());
        let tokens = result.unwrap().to_string();
        // Should be a no-op transition: |_ctx, _ev| { ActionResult::Ok }
        assert!(tokens.contains("_ctx"));
        assert!(tokens.contains("_ev"));
        assert!(tokens.contains("ActionResult"));
        // Should NOT contain a function call
        assert!(!tokens.contains("s_i"));
    }

    // ── Integration test: ping blox ──────────────────────────────────────────

    #[test]
    fn test_generate_concrete_spec_skeleton_ping() {
        // Parse the actual ping blox.toml
        let ping_toml = include_str!("../../../bloxes/ping/blox.toml");
        let blox_config: BloxConfig = toml::from_str(ping_toml).expect("parse ping blox.toml");

        // Generate the concrete spec skeleton (no impl crate for ping)
        let generated =
            generate_concrete_spec_skeleton(&blox_config, None, "ping-blox", "::ping_blox", None)
                .expect("generate");

        // ── Verify the generated code is valid Rust (parses with syn) ────────
        syn::parse_str::<syn::File>(&generated).expect("generated code should parse as valid Rust");

        // ── Verify header ────────────────────────────────────────────────────
        assert!(generated.contains("Auto-generated by bloxide-codegen"));

        // ── Verify spec struct and MachineSpec impl ──────────────────────────
        assert!(generated.contains("pub struct PingSpec"));
        assert!(generated.contains("MachineSpec"));
        assert!(generated.contains("for PingSpec"));
        assert!(generated.contains("type State = PingState"));
        assert!(generated.contains("type Event = PingEvent"));
        assert!(generated.contains("type Ctx = PingCtx"));
        assert!(generated.contains("PingState::Active"));

        // ── Verify concrete action closures (not stubs) ──────────────────────
        // increment_round: transition, fields = ["round:mut"], crate = blox_ctx_rounds
        assert!(
            generated.contains("blox_ctx_rounds"),
            "should reference blox_ctx_rounds crate"
        );
        assert!(
            generated.contains("increment_round"),
            "should call increment_round function"
        );
        // Should NOT have the stub comment for increment_round
        assert!(
            !generated.contains("stub: increment_round"),
            "increment_round should be a concrete call, not a stub"
        );

        // send_initial_ping: transition, fields = ["self_id", "peer_ref:ref", "round"],
        // crate = blox_ctx_ping_pong
        assert!(
            generated.contains("blox_ctx_ping_pong"),
            "should reference blox_ctx_ping_pong crate"
        );
        assert!(
            generated.contains("send_initial_ping"),
            "should call send_initial_ping function"
        );
        assert!(
            !generated.contains("stub: send_initial_ping"),
            "send_initial_ping should be a concrete call, not a stub"
        );

        // forward_ping: action name is "forward_ping" but fn_name = "send_ping"
        // The concrete code calls send_ping (the fn_name override)
        assert!(
            generated.contains("send_ping"),
            "should call send_ping function (fn_name override for forward_ping)"
        );
        assert!(
            !generated.contains("stub: forward_ping"),
            "forward_ping should be a concrete call, not a stub"
        );

        // schedule_pause_timer: action name is "schedule_pause_timer" but fn_name = "schedule_resume"
        assert!(
            generated.contains("blox_ctx_ping_pong"),
            "should reference blox_ctx_ping_pong crate"
        );
        assert!(
            generated.contains("schedule_resume"),
            "should call schedule_resume function (fn_name override for schedule_pause_timer)"
        );

        // cancel_pause_timer: fn_name = "cancel_timer_by_id" from bloxide_timer::actions
        assert!(
            generated.contains("::bloxide_timer::actions::cancel_timer_by_id"),
            "should reference bloxide_timer::actions::cancel_timer_by_id"
        );
        assert!(
            generated.contains("cancel_timer_by_id"),
            "should call cancel_timer_by_id function (fn_name override for cancel_pause_timer)"
        );

        // ── Verify field access expressions ──────────────────────────────────
        // round:mut → &mut ctx.round
        assert!(
            generated.contains("&mut ctx.round"),
            "should have &mut ctx.round field access"
        );
        // peer_ref:ref → &ctx.peer_ref
        assert!(
            generated.contains("&ctx.peer_ref"),
            "should have &ctx.peer_ref field access"
        );
        // self_id (no suffix) → ctx.self_id (copy/owned)
        assert!(
            generated.contains("ctx.self_id"),
            "should have ctx.self_id field access (copy/owned)"
        );

        // ── Verify NO stub closures remain for Self:: actions ─────────────────
        assert!(
            !generated.contains("stub:"),
            "no stub closures should remain in concrete spec"
        );

        // ── Verify topology is preserved ─────────────────────────────────────
        // State names appear in PingState:: variants and in the handler table name
        assert!(
            generated.contains("PingState::Active"),
            "should reference Active state"
        );
        assert!(
            generated.contains("PingState::Paused"),
            "should reference Paused state"
        );
        assert!(
            generated.contains("PingState::Error"),
            "should reference Error state"
        );
        assert!(
            generated.contains("ping_state_handler_table"),
            "should have handler table macro invocation"
        );
    }
}
