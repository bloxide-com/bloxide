// Copyright 2025 Bloxide, all rights reserved.
//! Generate MachineSpec skeleton from actor, topology, context, and event config.
//!
//! When `context.feature` is set, emits paired `#[cfg]` variants with different
//! generics, event types, and mailboxes types. The StateFns are generated as
//! associated constants inside `impl` blocks using raw `StateRule { ... }` struct
//! literals emitted directly from TOML.

use quote::{format_ident, quote, ToTokens};

use crate::schema::{ActorConfig, ContextConfig, EventConfig, TopologyConfig};
use crate::util::{to_snake_case, HEADER};

/// Parameters for a single variant of the spec skeleton.
struct VariantParams {
    /// The cfg attribute string (e.g. "not(feature = \"dynamic\")" or "feature = \"dynamic\"").
    /// None when no feature-gating.
    cfg_attr: Option<String>,
    /// The spec generics for this variant (e.g. `<R: BloxRuntime>` or `<R: BloxRuntime, B: SomeTrait + 'static>`).
    spec_generics: syn::Generics,
    /// The ctx type for this variant (e.g. `SupervisorCtx<R>` or `SupervisorCtx<R, F>`).
    ctx_ty: proc_macro2::TokenStream,
    /// The event type for this variant (e.g. `SupervisorEvent<R>` or `SupervisorEvent<R, F>`).
    event_ty: proc_macro2::TokenStream,
    /// The mailboxes type for this variant.
    mailboxes_ty: proc_macro2::TokenStream,
    /// The event type params as a list of idents (e.g. `["R"]` or `["R", "F"]`).
    event_type_params: Vec<String>,
    /// The ctx type params as a list of idents (e.g. `["R"]` or `["R", "F"]`).
    ctx_type_params: Vec<String>,
    /// Feature filter: None = non-feature transitions, Some(feat) = feature transitions.
    feature_filter: Option<String>,
    /// Extra imports specific to this variant (raw use paths).
    extra_imports: Vec<String>,
}

/// Replace placeholders {R}, {Ctx}, {Event} in a string with variant-specific values.
pub(crate) fn replace_placeholders(
    s: &str,
    ctx_type_str: &str,
    event_type_str: &str,
    type_params: &[String],
) -> String {
    let result = s.replace("{Ctx}", ctx_type_str);
    let result = result.replace("{Event}", event_type_str);
    // {R} → first type param (or "R" if no type params)
    let first_param = type_params.first().map(|s| s.as_str()).unwrap_or("R");
    result.replace("{R}", first_param)
}

/// Resolve an action string to a token stream.
///
/// When the action string starts with `Self::` (e.g. `Self::increment_round`),
/// generate a stub no-op closure instead of a function path reference.
/// The stub closures are no-ops that will be replaced by real implementations
/// in the system-level codegen. The stub body carries the action name in a
/// marker binding for readability.
///
/// For transition actions: `|_ctx, _ev| { let _stub = "<name>"; ::bloxide_core::transition::ActionResult::Ok }`
/// For entry/exit actions: `|_ctx| { let _stub = "<name>"; }`
///
/// Action strings that do NOT start with `Self::` (e.g. `handle_work_done` or
/// `bloxide_child_management::start_children`) are parsed as function path
/// references — these are real functions imported via `spec_imports`.
pub(crate) fn resolve_action(
    action: &str,
    ctx_type_str: &str,
    event_type_str: &str,
    type_params: &[String],
    _event_pattern: Option<&str>,
    is_transition: bool,
) -> anyhow::Result<proc_macro2::TokenStream> {
    let resolved = replace_placeholders(action, ctx_type_str, event_type_str, type_params);

    if resolved.starts_with("Self::") {
        // Self:: prefix indicates an action method on the spec struct.
        // In Phase 2, actions are stub no-op closures — the real action
        // implementations live in the blox crate's actions.rs (or context
        // crates) and are tested there, not through the generated spec.
        let name = resolved.strip_prefix("Self::").unwrap_or(&resolved);
        if is_transition {
            Ok(quote! {
                |_ctx, _ev| { let _stub = #name; ::bloxide_core::transition::ActionResult::Ok }
            })
        } else {
            Ok(quote! {
                |_ctx| { let _stub = #name; }
            })
        }
    } else {
        // Real function path reference — parse as a path.
        Ok(syn::parse_str::<syn::Path>(&resolved)
            .map(|p| p.to_token_stream())
            .unwrap_or_else(|_| {
                let ident = format_ident!("{}", resolved);
                quote! { #ident }
            }))
    }
}

/// Generate the StateFns associated constants for a variant.
///
/// Returns the impl-block tokens plus a flag telling whether a non-empty
/// `ROOT_RULES` const was emitted (after catch-all elision) — the caller uses
/// it to decide whether to generate the `root_transitions()` override.
#[allow(clippy::too_many_arguments)]
pub(crate) fn generate_state_fns_impl(
    topology: &TopologyConfig,
    event: Option<&EventConfig>,
    state_enum_ident: &syn::Ident,
    spec_ident: &syn::Ident,
    ctx_type_str: &str,
    event_type_str: &str,
    type_params: &[String],
    spec_impl_generics: proc_macro2::TokenStream,
    spec_ty_generics: proc_macro2::TokenStream,
    spec_where_clause: Option<&syn::WhereClause>,
    feature_filter: Option<&str>,
    strip_feature_cfg: bool,
    action_resolver: crate::ActionResolver<'_>,
) -> anyhow::Result<(proc_macro2::TokenStream, bool)> {
    use crate::schema::{EntryExitConfig, TransitionConfig, ROOT_STATE_KEYWORD};
    use crate::topology::generate_state_rule;
    use std::collections::HashMap;

    // Partition transitions: state rules (owned by a user state) vs root rules
    // (state = "root" — VirtualRoot fallback, emitted as ROOT_RULES below).
    // Build lookup maps, filtering by feature
    let trans_by_state: HashMap<String, Vec<&TransitionConfig>> = {
        let mut map: HashMap<String, Vec<&TransitionConfig>> = HashMap::new();
        for t in &topology.transitions {
            if t.state == ROOT_STATE_KEYWORD {
                continue;
            }
            // Non-feature variant: include transitions with no `feature` attribute.
            // Feature variant: include ALL transitions (both gated and non-gated).
            if match feature_filter {
                None => t.feature.is_none(),
                Some(_) => true,
            } {
                map.entry(t.state.clone()).or_default().push(t);
            }
        }
        map
    };
    // Root rules for this variant, with the same feature filtering.
    let root_trans: Vec<&TransitionConfig> = topology
        .transitions
        .iter()
        .filter(|t| t.state == ROOT_STATE_KEYWORD)
        .filter(|t| match feature_filter {
            None => t.feature.is_none(),
            Some(_) => true,
        })
        .collect();
    let entry_by_state: HashMap<String, Vec<&EntryExitConfig>> = {
        let mut map: HashMap<String, Vec<&EntryExitConfig>> = HashMap::new();
        for e in &topology.entry {
            // Same feature filtering as transitions
            if match feature_filter {
                None => e.feature.is_none(),
                Some(_) => true,
            } {
                map.entry(e.state.clone()).or_default().push(e);
            }
        }
        map
    };
    let exit_by_state: HashMap<String, Vec<&EntryExitConfig>> = {
        let mut map: HashMap<String, Vec<&EntryExitConfig>> = HashMap::new();
        for e in &topology.exit {
            if match feature_filter {
                None => e.feature.is_none(),
                Some(_) => true,
            } {
                map.entry(e.state.clone()).or_default().push(e);
            }
        }
        map
    };

    let mut consts = Vec::new();
    for state in &topology.states {
        let fns_ident = format_ident!("{}_FNS", to_snake_case(&state.name).to_ascii_uppercase());
        let state_trans = trans_by_state
            .get(&state.name)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let state_entry = entry_by_state
            .get(&state.name)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let state_exit = exit_by_state
            .get(&state.name)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        // on_entry actions
        let entry_tokens: Vec<proc_macro2::TokenStream> = state_entry
            .first()
            .map(|ee| {
                ee.actions
                    .iter()
                    .map(|a| {
                        action_resolver(a, ctx_type_str, event_type_str, type_params, None, false)
                    })
                    .collect::<anyhow::Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        // on_exit actions
        let exit_tokens: Vec<proc_macro2::TokenStream> = state_exit
            .first()
            .map(|ee| {
                ee.actions
                    .iter()
                    .map(|a| {
                        action_resolver(a, ctx_type_str, event_type_str, type_params, None, false)
                    })
                    .collect::<anyhow::Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        // Transition rules — raw StateRule { ... } literals. Catch-all rules
        // made unreachable by earlier rules covering every declared variant
        // of the mailbox's message enum are omitted (see
        // `topology::catchall_elision`).
        let elide = crate::topology::catchall_elision(state_trans, event);
        let rules: Vec<proc_macro2::TokenStream> = state_trans
            .iter()
            .zip(elide.iter())
            .filter(|(_, elided)| !**elided)
            .map(|(t, _)| {
                generate_state_rule(
                    t,
                    state_enum_ident,
                    ctx_type_str,
                    event_type_str,
                    type_params,
                    action_resolver,
                    strip_feature_cfg,
                    feature_filter,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let trans_tokens: proc_macro2::TokenStream = if rules.is_empty() {
            quote! { &[] }
        } else {
            quote! { &[#(#rules),*] }
        };

        consts.push(quote! {
            #[allow(unused_variables)]
            const #fns_ident: ::bloxide_core::spec::StateFns<Self> = ::bloxide_core::spec::StateFns {
                on_entry: &[#(#entry_tokens),*],
                on_exit: &[#(#exit_tokens),*],
                transitions: #trans_tokens,
            };
        });
    }

    // Root-level transition rules (VirtualRoot fallback for domain events).
    // One associated const per spec (not per state), holding this variant's
    // root partition. Referenced by the generated `root_transitions()`
    // override when non-empty (after catch-all elision).
    let root_elide = crate::topology::catchall_elision(&root_trans, event);
    let root_rules: Vec<proc_macro2::TokenStream> = root_trans
        .iter()
        .zip(root_elide.iter())
        .filter(|(_, elided)| !**elided)
        .map(|(t, _)| {
            generate_state_rule(
                t,
                state_enum_ident,
                ctx_type_str,
                event_type_str,
                type_params,
                action_resolver,
                strip_feature_cfg,
                feature_filter,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let has_root_rules = !root_rules.is_empty();
    if has_root_rules {
        consts.push(quote! {
            #[allow(unused_variables)]
            const ROOT_RULES: &'static [::bloxide_core::transition::StateRule<Self>] = &[#(#root_rules),*];
        });
    }

    Ok((
        quote! {
            impl #spec_impl_generics #spec_ident #spec_ty_generics #spec_where_clause {
                #(#consts)*
            }
        },
        has_root_rules,
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn generate(
    actor: &ActorConfig,
    topology: &TopologyConfig,
    context: &ContextConfig,
    event: Option<&EventConfig>,
    _crate_name: &str,
    blox_crate_path: &str,
    action_resolver: crate::ActionResolver<'_>,
    active_feature: Option<&str>,
) -> anyhow::Result<String> {
    let actor_name = &actor.name;
    let spec_ident = format_ident!("{}", format!("{}Spec", actor_name));
    let state_ident = format_ident!("{}", format!("{}State", actor_name));

    // Track whether this is a system-level spec (blox_crate_path starts with "::").
    // For blox-level generation this is "crate"; for system-level it's e.g. "::ping_blox".
    let is_system_level = blox_crate_path.starts_with("::");

    // Save the string form before parsing into syn::Path.
    let blox_crate_path_str = blox_crate_path.to_string();

    // Parse the blox_crate_path into a syn::Path for use in quote! macros.
    let blox_crate_path: syn::Path = syn::parse_str(blox_crate_path)
        .map_err(|e| anyhow::anyhow!("invalid blox_crate_path '{}': {}", blox_crate_path, e))?;

    // Helper: translate a spec_import path that starts with `crate::` to use
    // blox_crate_path instead. Only replaces the leading `crate::`, not
    // occurrences in the middle of a path.
    let translate_crate_import = |imp: &str| -> String {
        if let Some(rest) = imp.strip_prefix("crate::") {
            format!("{}::{}", blox_crate_path_str, rest)
        } else if imp == "crate" {
            blox_crate_path_str.clone()
        } else {
            imp.to_string()
        }
    };

    let handler_macro_name_str = format!(
        "{}_handler_table",
        to_snake_case(&format!("{}State", actor_name))
    );
    let handler_macro_ident = format_ident!("{}", handler_macro_name_str);

    let ctx_ident = format_ident!("{}", context.name);

    // ── Determine if feature-gated ──────────────────────────────────────────
    let has_feature = context.feature.is_some();

    // ── Collect shared data ──────────────────────────────────────────────────
    let initial_state = topology
        .states
        .iter()
        .find(|s| s.initial.unwrap_or(false))
        .map(|s| s.name.clone())
        .or_else(|| {
            topology
                .states
                .iter()
                .find(|s| !s.composite.unwrap_or(false))
                .map(|s| s.name.clone())
        });
    let initial_state_ident = if let Some(ref name) = initial_state {
        format_ident!("{}", name)
    } else {
        format_ident!("Init")
    };

    let error_states: Vec<_> = topology
        .states
        .iter()
        .filter(|s| s.error.unwrap_or(false))
        .map(|s| format_ident!("{}", s.name))
        .collect();

    let (is_error_param, is_error_body) = if error_states.is_empty() {
        (quote! { _state }, quote! { false })
    } else {
        (
            quote! { state },
            quote! { ::core::matches!(state, #state_ident::#(#error_states)|*) },
        )
    };

    // ── Build imports ─────────────────────────────────────────────────────────
    let mut use_stmts = vec![quote! { use ::core::marker::PhantomData; }];

    // BloxRuntime import — needed when generics include it
    let context_has_runtime = context
        .generics
        .as_ref()
        .map(|g| g.contains("BloxRuntime"))
        .unwrap_or(false);
    let event_generics_str: Option<&str> = event
        .and_then(|e| e.generics.as_deref())
        .or(context.event_generics.as_deref());
    let event_has_runtime = event_generics_str
        .map(|g| g.contains("BloxRuntime"))
        .unwrap_or(false);
    if context_has_runtime || event_has_runtime {
        use_stmts.push(quote! { use ::bloxide_core::capability::BloxRuntime; });
    }
    use_stmts.push(quote! { use ::bloxide_core::spec::{MachineSpec, StateFns}; });
    use_stmts.push(quote! { use #blox_crate_path::#ctx_ident; });
    use_stmts.push(quote! { pub use #blox_crate_path::generated::topology::#state_ident; });

    // Import the handler table macro from the blox crate when generating
    // system-level specs. For blox-level specs, the macro is already in scope
    // via #[macro_use] mod topology.
    if is_system_level {
        use_stmts.push(quote! {
            #[allow(unused_imports)]
            use #blox_crate_path::#handler_macro_ident;
        });
    }

    // Event import
    let event_name_str = event
        .map(|e| e.name.clone())
        .or_else(|| context.event_name.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "spec_skeleton requires either an [event] section or context.event_name"
            )
        })?;
    let event_ident = format_ident!("{}", event_name_str);
    // The event is imported from the crate root, but for hand-written events
    // it may be in a different crate. We use `use crate::#event_ident` as the
    // default. The blox.toml can override this with explicit imports.
    // Actually, the old code had `use crate::{SupervisorCtx, SupervisorEvent};`
    // which means the event is re-exported at the crate root.
    // For hand-written events, the blox crate re-exports the event from the
    // context crate. We emit a combined import.
    use_stmts.push(quote! { use #blox_crate_path::#event_ident; });

    // Add spec_imports from topology (raw use statements for action functions)
    // Translate leading `crate::` prefix to blox_crate_path for system-level codegen.
    for imp in &topology.spec_imports {
        let translated = translate_crate_import(imp);
        let use_item: syn::ItemUse = syn::parse_str(&format!("use {};", translated))
            .map_err(|e| anyhow::anyhow!("invalid spec_import '{}': {}", translated, e))?;
        use_stmts.push(quote! {
            #[allow(unused_imports)]
            #use_item
        });
    }

    // Envelope is needed when FullEvent patterns match on Envelope-wrapped
    // variants (e.g. `SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))`).
    // Always import it when there's an [event] section — it's harmless if unused.
    if event.is_some() {
        use_stmts.push(quote! {
            #[allow(unused_imports)]
            use ::bloxide_core::messaging::Envelope;
        });
    }

    // Add message type imports — the generated matches closures reference
    // message enum types (e.g. `CounterMsg::Tick(_)`) which must be in scope.
    // Collect unique crate-level import paths from mailbox message_path fields.
    let mut msg_import_paths: Vec<String> = Vec::new();
    if let Some(ev) = event {
        for mb in &ev.mailboxes {
            if let Some(ref mp) = mb.message_path {
                // message_path is like "counter_messages::CounterMsg" or
                // "pool_messages::WorkerCtrl<R>" — strip generics, take
                // the crate::path part as a single use statement.
                let base = mp.split('<').next().unwrap_or(mp).trim();
                if !msg_import_paths.iter().any(|p| p == base) {
                    msg_import_paths.push(base.to_string());
                }
            }
        }
    }
    for path in &msg_import_paths {
        // Translate leading `crate::` to blox_crate_path for system-level codegen.
        let translated = if path.starts_with("crate::") && blox_crate_path_str != "crate" {
            format!("{}::{}", blox_crate_path_str, &path["crate::".len()..])
        } else {
            path.clone()
        };
        let use_item: syn::ItemUse = syn::parse_str(&format!("use {};", translated))
            .map_err(|e| anyhow::anyhow!("invalid message import '{}': {}", translated, e))?;
        use_stmts.push(quote! {
            #[allow(unused_imports)]
            #use_item
        });
    }

    // ── Build variants ─────────────────────────────────────────────────────────
    let variants = if has_feature {
        let feat_name = context.feature.as_deref().unwrap();

        // Non-feature variant
        let base_generics =
            syn::parse_str::<syn::Generics>(context.generics.as_deref().unwrap_or("<>"))
                .map_err(|e| anyhow::anyhow!("invalid generics: {}", e))?;

        let base_event_generics = if let Some(ev) = event {
            syn::parse_str::<syn::Generics>(ev.generics.as_deref().unwrap_or("<>"))
                .map_err(|e| anyhow::anyhow!("invalid [event].generics: {}", e))?
        } else {
            syn::parse_str::<syn::Generics>(context.event_generics.as_deref().unwrap_or("<>"))
                .map_err(|e| anyhow::anyhow!("invalid event_generics: {}", e))?
        };

        let base_ctx_params: Vec<String> = base_generics
            .type_params()
            .map(|tp| tp.ident.to_string())
            .collect();
        let base_event_params: Vec<String> = base_event_generics
            .type_params()
            .map(|tp| tp.ident.to_string())
            .collect();

        let base_ctx_ty = if base_ctx_params.is_empty() {
            quote! { #ctx_ident }
        } else {
            let params: Vec<_> = base_generics.type_params().map(|tp| &tp.ident).collect();
            quote! { #ctx_ident<#(#params),*> }
        };

        let base_event_ty = if base_event_params.is_empty() {
            quote! { #event_ident }
        } else {
            let params: Vec<_> = base_event_generics
                .type_params()
                .map(|tp| &tp.ident)
                .collect();
            quote! { #event_ident<#(#params),*> }
        };

        // Derive base mailboxes type from [event] section when present,
        // falling back to context.mailboxes_type for hand-written events.
        let base_mailboxes_ty: proc_macro2::TokenStream = if let Some(ev) = event {
            let mailboxes_tys: Vec<_> = ev
                .mailboxes
                .iter()
                .filter(|mb| mb.feature.is_none())
                .map(|mb| {
                    let msg_type = if let Some(ref path) = mb.message_path {
                        // Translate `crate::` prefix to blox_crate_path for
                        // system-level codegen.
                        let translated = translate_crate_import(path);
                        syn::parse_str::<syn::Path>(&translated).map_err(|e| {
                            anyhow::anyhow!("invalid message_path '{}': {}", translated, e)
                        })?
                    } else {
                        syn::parse_str::<syn::Path>(&mb.message).map_err(|e| {
                            anyhow::anyhow!("invalid message '{}': {}", mb.message, e)
                        })?
                    };
                    Ok::<_, anyhow::Error>(quote! { Rt::Stream<#msg_type> })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            quote! { (#(#mailboxes_tys,)*) }
        } else {
            let base_mailboxes = context.mailboxes_type.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "context.mailboxes_type required for feature-gated specs without [event] section"
                )
            })?;
            base_mailboxes.parse().map_err(|e| {
                anyhow::anyhow!("invalid mailboxes_type '{}': {}", base_mailboxes, e)
            })?
        };

        // Build spec generics for base variant
        let mut base_spec_generics =
            syn::parse_str::<syn::Generics>(context.generics.as_deref().unwrap_or("<>"))
                .map_err(|e| anyhow::anyhow!("invalid generics: {}", e))?;
        // Add extra_where
        if !context.extra_where.is_empty() {
            let mut preds = syn::punctuated::Punctuated::new();
            if let Some(ref wc) = base_spec_generics.where_clause {
                for pred in wc.predicates.iter() {
                    let pred_str = pred.to_token_stream().to_string();
                    preds.push(
                        syn::parse_str::<syn::WherePredicate>(&pred_str)
                            .map_err(|e| anyhow::anyhow!("re-parse where predicate: {}", e))?,
                    );
                }
            }
            for w in &context.extra_where {
                preds.push(
                    syn::parse_str::<syn::WherePredicate>(w)
                        .map_err(|e| anyhow::anyhow!("invalid extra_where '{}': {}", w, e))?,
                );
            }
            base_spec_generics.where_clause = Some(syn::WhereClause {
                where_token: syn::Token![where](proc_macro2::Span::call_site()),
                predicates: preds,
            });
        }

        // Feature variant
        let feature_generics =
            syn::parse_str::<syn::Generics>(context.feature_generics.as_deref().unwrap_or("<>"))
                .map_err(|e| anyhow::anyhow!("invalid feature_generics: {}", e))?;

        let feature_event_generics = if let Some(ev) = event {
            syn::parse_str::<syn::Generics>(ev.feature_generics.as_deref().unwrap_or("<>"))
                .map_err(|e| anyhow::anyhow!("invalid [event].feature_generics: {}", e))?
        } else {
            syn::parse_str::<syn::Generics>(
                context.feature_event_generics.as_deref().unwrap_or("<>"),
            )
            .map_err(|e| anyhow::anyhow!("invalid feature_event_generics: {}", e))?
        };

        let feature_ctx_params: Vec<String> = feature_generics
            .type_params()
            .map(|tp| tp.ident.to_string())
            .collect();
        let feature_event_params: Vec<String> = feature_event_generics
            .type_params()
            .map(|tp| tp.ident.to_string())
            .collect();

        let feature_ctx_ty = if feature_ctx_params.is_empty() {
            quote! { #ctx_ident }
        } else {
            let params: Vec<_> = feature_generics.type_params().map(|tp| &tp.ident).collect();
            quote! { #ctx_ident<#(#params),*> }
        };

        let feature_event_ty = if feature_event_params.is_empty() {
            quote! { #event_ident }
        } else {
            let params: Vec<_> = feature_event_generics
                .type_params()
                .map(|tp| &tp.ident)
                .collect();
            quote! { #event_ident<#(#params),*> }
        };

        // Derive feature mailboxes type from [event] section when present
        // (all mailboxes, including feature-gated ones), falling back to
        // context.feature_mailboxes_type for hand-written events.
        let feature_mailboxes_ty: proc_macro2::TokenStream = if let Some(ev) = event {
            let mailboxes_tys: Vec<_> = ev
                .mailboxes
                .iter()
                .map(|mb| {
                    let msg_type = if let Some(ref path) = mb.message_path {
                        // Translate `crate::` prefix to blox_crate_path for
                        // system-level codegen.
                        let translated = translate_crate_import(path);
                        syn::parse_str::<syn::Path>(&translated).map_err(|e| {
                            anyhow::anyhow!("invalid message_path '{}': {}", translated, e)
                        })?
                    } else {
                        syn::parse_str::<syn::Path>(&mb.message).map_err(|e| {
                            anyhow::anyhow!("invalid message '{}': {}", mb.message, e)
                        })?
                    };
                    Ok::<_, anyhow::Error>(quote! { Rt::Stream<#msg_type> })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            quote! { (#(#mailboxes_tys,)*) }
        } else {
            let feature_mailboxes = context.feature_mailboxes_type.as_deref().ok_or_else(|| {
                anyhow::anyhow!("context.feature_mailboxes_type required for feature-gated specs")
            })?;
            feature_mailboxes.parse().map_err(|e| {
                anyhow::anyhow!(
                    "invalid feature_mailboxes_type '{}': {}",
                    feature_mailboxes,
                    e
                )
            })?
        };

        // Build spec generics for feature variant
        let mut feature_spec_generics =
            syn::parse_str::<syn::Generics>(context.feature_generics.as_deref().unwrap_or("<>"))
                .map_err(|e| anyhow::anyhow!("invalid feature_generics: {}", e))?;
        // Add 'static to feature-variant type params that need it.
        // Feature variants may need 'static bounds on extra type params
        // (beyond R: BloxRuntime). Use feature_where for this.
        if !context.feature_where.is_empty() {
            let mut preds = syn::punctuated::Punctuated::new();
            if let Some(ref wc) = feature_spec_generics.where_clause {
                for pred in wc.predicates.iter() {
                    let pred_str = pred.to_token_stream().to_string();
                    preds.push(
                        syn::parse_str::<syn::WherePredicate>(&pred_str)
                            .map_err(|e| anyhow::anyhow!("re-parse where predicate: {}", e))?,
                    );
                }
            }
            for w in &context.feature_where {
                preds.push(
                    syn::parse_str::<syn::WherePredicate>(w)
                        .map_err(|e| anyhow::anyhow!("invalid feature_where '{}': {}", w, e))?,
                );
            }
            feature_spec_generics.where_clause = Some(syn::WhereClause {
                where_token: syn::Token![where](proc_macro2::Span::call_site()),
                predicates: preds,
            });
        }

        vec![
            VariantParams {
                cfg_attr: Some(format!("not(feature = \"{}\")", feat_name)),
                spec_generics: base_spec_generics,
                ctx_ty: base_ctx_ty,
                event_ty: base_event_ty,
                mailboxes_ty: base_mailboxes_ty,
                event_type_params: base_event_params,
                ctx_type_params: base_ctx_params,
                feature_filter: None,
                extra_imports: Vec::new(),
            },
            VariantParams {
                cfg_attr: Some(format!("feature = \"{}\"", feat_name)),
                spec_generics: feature_spec_generics,
                ctx_ty: feature_ctx_ty,
                event_ty: feature_event_ty,
                mailboxes_ty: feature_mailboxes_ty,
                event_type_params: feature_event_params,
                ctx_type_params: feature_ctx_params,
                feature_filter: Some(feat_name.to_string()),
                extra_imports: topology
                    .feature_spec_imports
                    .iter()
                    .map(|imp| translate_crate_import(imp))
                    .collect(),
            },
        ]
    } else {
        // Single variant (no feature-gating)
        let ctx_generics = context
            .generics
            .as_ref()
            .map(|g| {
                syn::parse_str::<syn::Generics>(g)
                    .map_err(|e| anyhow::anyhow!("invalid context generics '{}': {}", g, e))
            })
            .transpose()?
            .unwrap_or_default();

        let event_generics = event_generics_str
            .map(|g| {
                syn::parse_str::<syn::Generics>(g)
                    .map_err(|e| anyhow::anyhow!("invalid event generics '{}': {}", g, e))
            })
            .transpose()?
            .unwrap_or_default();

        let (_event_impl_generics, event_ty_generics, _event_where_clause) =
            event_generics.split_for_impl();

        let ctx_params: Vec<String> = ctx_generics
            .type_params()
            .map(|tp| tp.ident.to_string())
            .collect();
        let event_params: Vec<String> = event_generics
            .type_params()
            .map(|tp| tp.ident.to_string())
            .collect();

        let ctx_ty = if ctx_generics.type_params().count() > 0 {
            let (_impl_g, ty_g, _where_g) = ctx_generics.split_for_impl();
            quote! { #ctx_ident #ty_g }
        } else {
            quote! { #ctx_ident }
        };

        let event_ty = if event_generics.type_params().count() > 0 {
            quote! { #event_ident #event_ty_generics }
        } else {
            quote! { #event_ident }
        };

        // Mailboxes
        let mailboxes_ty = if let Some(event) = event {
            let mailboxes_tys: Vec<_> = event
                .mailboxes
                .iter()
                .map(|mb| {
                    let msg_type = if let Some(ref path) = mb.message_path {
                        // Translate `crate::` prefix to blox_crate_path for
                        // system-level codegen.
                        let translated = translate_crate_import(path);
                        syn::parse_str::<syn::Path>(&translated).map_err(|e| {
                            anyhow::anyhow!("invalid message_path '{}': {}", translated, e)
                        })?
                    } else {
                        syn::parse_str::<syn::Path>(&mb.message).map_err(|e| {
                            anyhow::anyhow!("invalid message '{}': {}", mb.message, e)
                        })?
                    };
                    Ok(quote! { Rt::Stream<#msg_type> })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            quote! { (#(#mailboxes_tys,)*) }
        } else if let Some(ref mb_type) = context.mailboxes_type {
            syn::parse_str::<proc_macro2::TokenStream>(mb_type)
                .map_err(|e| anyhow::anyhow!("invalid mailboxes_type '{}': {}", mb_type, e))?
        } else {
            anyhow::bail!(
                "spec_skeleton requires either [event].mailboxes or context.mailboxes_type"
            )
        };

        // Build spec generics
        let mut spec_generics =
            syn::parse_str::<syn::Generics>(context.generics.as_deref().unwrap_or("<>"))
                .map_err(|e| anyhow::anyhow!("invalid generics: {}", e))?;
        // Add extra_where
        if !context.extra_where.is_empty() {
            let mut preds = syn::punctuated::Punctuated::new();
            if let Some(ref wc) = spec_generics.where_clause {
                for pred in wc.predicates.iter() {
                    let pred_str = pred.to_token_stream().to_string();
                    preds.push(
                        syn::parse_str::<syn::WherePredicate>(&pred_str)
                            .map_err(|e| anyhow::anyhow!("re-parse where predicate: {}", e))?,
                    );
                }
            }
            for w in &context.extra_where {
                preds.push(
                    syn::parse_str::<syn::WherePredicate>(w)
                        .map_err(|e| anyhow::anyhow!("invalid extra_where '{}': {}", w, e))?,
                );
            }
            spec_generics.where_clause = Some(syn::WhereClause {
                where_token: syn::Token![where](proc_macro2::Span::call_site()),
                predicates: preds,
            });
        }

        vec![VariantParams {
            cfg_attr: None,
            spec_generics,
            ctx_ty,
            event_ty,
            mailboxes_ty,
            event_type_params: event_params,
            ctx_type_params: ctx_params,
            feature_filter: None,
            extra_imports: Vec::new(),
        }]
    };

    // ── Generate each variant ─────────────────────────────────────────────────
    // Root-level transitions (state = "root"): a variant emits the
    // `root_transitions()` override only when its feature-filtered partition
    // still contains at least one root rule after catch-all elision. The rules
    // live in the `ROOT_RULES` associated const emitted by
    // `generate_state_fns_impl`, which reports the post-elision emptiness.

    let mut variant_tokens = Vec::new();

    for var in &variants {
        // When active_feature is set (system-level codegen), skip variants
        // that don't match the active feature. This strips #[cfg] gates and
        // emits only the active variant, since the feature is selected in
        // the blox crate's Cargo.toml dependency, not the app crate.
        if let Some(active) = active_feature {
            let matches = match &var.cfg_attr {
                None => true, // non-feature variant — only if no feature gate at all
                Some(cfg) => cfg == &format!("feature = \"{}\"", active),
            };
            if !matches {
                continue;
            }
        }
        let (spec_impl_generics, spec_ty_generics, spec_where_clause) =
            var.spec_generics.split_for_impl();

        // PhantomData type
        let phantom_types = if var.ctx_type_params.is_empty() {
            quote! { () }
        } else if var.ctx_type_params.len() == 1 {
            let p = format_ident!("{}", var.ctx_type_params[0]);
            quote! { #p }
        } else {
            let params: Vec<_> = var
                .ctx_type_params
                .iter()
                .map(|s| format_ident!("{}", s))
                .collect();
            quote! { (#(#params),*) }
        };

        // Ctx and Event type strings for placeholder replacement
        let ctx_type_str = var.ctx_ty.to_token_stream().to_string();
        let event_type_str = var.event_ty.to_token_stream().to_string();
        // Clean up whitespace from to_string()
        let ctx_type_str = ctx_type_str.replace(" ", "");
        let event_type_str = event_type_str.replace(" ", "");

        // Spec struct
        let struct_def = quote! {
            pub struct #spec_ident #spec_impl_generics #spec_where_clause {
                _phantom: PhantomData<#phantom_types>,
            }
        };

        // StateFns associated constants — always generated as associated constants
        // inside the impl block using raw StateRule literals. This handles both
        // feature-gated and non-feature-gated specs uniformly.
        let (state_fns_impl, has_root_rules) = generate_state_fns_impl(
            topology,
            event,
            &state_ident,
            &spec_ident,
            &ctx_type_str,
            &event_type_str,
            &var.event_type_params,
            spec_impl_generics.to_token_stream(),
            spec_ty_generics.to_token_stream(),
            spec_where_clause,
            var.feature_filter.as_deref(),
            active_feature.is_some(),
            action_resolver,
        )?;

        // MachineSpec impl
        let root_transitions_fn = if has_root_rules {
            quote! {
                fn root_transitions() -> &'static [::bloxide_core::transition::StateRule<Self>] {
                    Self::ROOT_RULES
                }
            }
        } else {
            quote! {}
        };
        // on_init body — per-variant: the feature variant resets feature-gated
        // state fields via `feature_on_init` (falling back to `on_init`).
        let on_init_src = if var.feature_filter.is_some() {
            context
                .feature_on_init
                .as_ref()
                .or(context.on_init.as_ref())
        } else {
            context.on_init.as_ref()
        };
        let on_init_body: proc_macro2::TokenStream = if let Some(body) = on_init_src {
            body.parse::<proc_macro2::TokenStream>()
                .map_err(|e| anyhow::anyhow!("invalid on_init body '{}': {}", body, e))?
        } else {
            quote! {}
        };
        let on_init_fn = if on_init_body.is_empty() {
            quote! { fn on_init_entry(_ctx: &mut Self::Ctx) {} }
        } else {
            quote! {
                fn on_init_entry(ctx: &mut Self::Ctx) {
                    #on_init_body
                }
            }
        };

        let var_event_ty = &var.event_ty;
        let var_ctx_ty = &var.ctx_ty;
        let var_mailboxes_ty = &var.mailboxes_ty;
        let impl_block = quote! {
            impl #spec_impl_generics MachineSpec for #spec_ident #spec_ty_generics #spec_where_clause {
                type State = #state_ident;
                type Event = #var_event_ty;
                type Ctx = #var_ctx_ty;
                type Mailboxes<Rt: ::bloxide_core::capability::BloxRuntime> = #var_mailboxes_ty;

                const HANDLER_TABLE: &'static [&'static StateFns<Self>] = #handler_macro_ident!(Self);

                fn initial_state() -> #state_ident {
                    #state_ident::#initial_state_ident
                }

                fn is_error(#is_error_param: &#state_ident) -> bool {
                    #is_error_body
                }

                #on_init_fn

                #root_transitions_fn
            }
        };

        // Extra imports for this variant
        let extra_import_tokens: Vec<proc_macro2::TokenStream> = var
            .extra_imports
            .iter()
            .map(|imp| {
                let use_item: syn::ItemUse = syn::parse_str(&format!("use {};", imp))
                    .map_err(|e| anyhow::anyhow!("invalid extra_import '{}': {}", imp, e))?;
                Ok(quote! {
                    #[allow(unused_imports)]
                    #use_item
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        // Wrap each item in #[cfg] if needed. When active_feature is set,
        // we've already filtered to only the active variant, so strip #[cfg].
        let wrapped = if let Some(ref cfg) = var.cfg_attr {
            if active_feature.is_some() {
                // System-level: feature already selected via Cargo.toml, no #[cfg] needed.
                quote! {
                    #(#extra_import_tokens)*
                    #struct_def
                    #state_fns_impl
                    #impl_block
                }
            } else {
                let cfg_ts: proc_macro2::TokenStream = cfg
                    .parse()
                    .map_err(|e| anyhow::anyhow!("invalid cfg attr '{}': {}", cfg, e))?;
                // Attach #[cfg] to each extra import individually — a bare
                // `#[cfg] #(#imports)* #[cfg] #struct` sequence would collapse
                // onto the struct when imports are empty, duplicating the
                // attribute (clippy::duplicated_attributes).
                let cfgd_imports: Vec<_> = extra_import_tokens
                    .iter()
                    .map(|imp| quote! { #[cfg(#cfg_ts)] #imp })
                    .collect();
                quote! {
                    #(#cfgd_imports)*
                    #[cfg(#cfg_ts)]
                    #struct_def
                    #[cfg(#cfg_ts)]
                    #state_fns_impl
                    #[cfg(#cfg_ts)]
                    #impl_block
                }
            }
        } else {
            quote! {
                #(#extra_import_tokens)*
                #struct_def
                #state_fns_impl
                #impl_block
            }
        };

        variant_tokens.push(wrapped);
    }

    // ── Assemble final output ─────────────────────────────────────────────────
    let tokens = quote! {
        #(#use_stmts)*
        #(#variant_tokens)*
    };

    let raw = tokens.to_string();
    let file = syn::parse_str::<syn::File>(&raw)
        .map_err(|e| anyhow::anyhow!("syn parsing failed: {}", e))?;
    let formatted = prettyplease::unparse(&file);

    Ok(format!("{}{}", HEADER, formatted))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::BloxConfig;

    /// Counter blox without root transitions (mirrors crates/bloxes/counter/blox.toml).
    const COUNTER_TOML: &str = r#"
[actor]
name = "Counter"

[context]
name = "CounterCtx"
on_init = "ctx.count = 0;"

[[context.fields]]
name = "count"
type = "u32"

[[context.actions]]
name = "count_tick"
fn_name = "increment_count"
crate = "blox_ctx_ticks"
fields = ["count:mut"]
impl_required = false

[event]
name = "CounterEvent"

[[event.mailboxes]]
variant = "Msg"
message = "CounterMsg"
message_path = "counter_messages::CounterMsg"

[topology]
spec_imports = ["crate::DONE_AT_COUNT"]

[[topology.states]]
name = "Ready"
initial = true

[[topology.transitions]]
state = "Ready"
event = "CounterMsg::Tick(_)"
target = "stay"
actions = ["Self::count_tick"]

[[topology.transitions.guards]]
condition = "ctx.count >= DONE_AT_COUNT"
target = "done"
"#;

    /// Same blox plus a root-level rule (`state = "root"` — VirtualRoot
    /// fallback for domain events).
    const COUNTER_ROOT_TOML: &str = r#"
[actor]
name = "Counter"

[context]
name = "CounterCtx"
on_init = "ctx.count = 0;"

[[context.fields]]
name = "count"
type = "u32"

[[context.actions]]
name = "count_tick"
fn_name = "increment_count"
crate = "blox_ctx_ticks"
fields = ["count:mut"]
impl_required = false

[[context.actions]]
name = "note_unhandled"
crate = "blox_ctx_ticks"
fields = ["count"]
impl_required = false

[event]
name = "CounterEvent"

[[event.mailboxes]]
variant = "Msg"
message = "CounterMsg"
message_path = "counter_messages::CounterMsg"

[topology]
spec_imports = ["crate::DONE_AT_COUNT"]

[[topology.states]]
name = "Ready"
initial = true

[[topology.transitions]]
state = "Ready"
event = "CounterMsg::Tick(_)"
target = "stay"
actions = ["Self::count_tick"]

[[topology.transitions.guards]]
condition = "ctx.count >= DONE_AT_COUNT"
target = "done"

[[topology.transitions]]
state = "root"
event = "CounterMsg::PoisonPill(_)"
target = "reset"
actions = ["Self::note_unhandled"]
"#;

    fn parse(toml_str: &str) -> BloxConfig {
        toml::from_str(toml_str).expect("test blox.toml must parse")
    }

    fn generate_stub(config: &BloxConfig) -> String {
        generate(
            config.actor.as_ref().unwrap(),
            config.topology.as_ref().unwrap(),
            config.context.as_ref().unwrap(),
            config.event.as_ref(),
            "counter-blox",
            "crate",
            &resolve_action,
            None,
        )
        .expect("stub generation must succeed")
    }

    #[test]
    fn root_rules_parse_as_plain_transitions() {
        let config = parse(COUNTER_ROOT_TOML);
        let transitions = &config.topology.as_ref().unwrap().transitions;
        let root: Vec<_> = transitions.iter().filter(|t| t.state == "root").collect();
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].event, "CounterMsg::PoisonPill(_)");
        assert_eq!(root[0].target, "reset");
        assert_eq!(root[0].actions, vec!["Self::note_unhandled".to_string()]);
        // Defaults to none when no state = "root" rule is present.
        let config = parse(COUNTER_TOML);
        assert!(!config
            .topology
            .as_ref()
            .unwrap()
            .transitions
            .iter()
            .any(|t| t.state == "root"));
    }

    #[test]
    fn no_root_transitions_emits_no_override() {
        let out = generate_stub(&parse(COUNTER_TOML));
        assert!(!out.contains("root_transitions"));
        assert!(!out.contains("ROOT_RULES"));
    }

    #[test]
    fn stub_generation_emits_root_rules() {
        let out = generate_stub(&parse(COUNTER_ROOT_TOML));
        assert!(
            out.contains("fn root_transitions()"),
            "missing override:\n{out}"
        );
        assert!(out.contains("ROOT_RULES"), "missing rules const:\n{out}");
        assert!(
            out.contains("Self::ROOT_RULES"),
            "override must reference the const:\n{out}"
        );
        // Msg-shorthand pattern: wildcard tag + msg_payload match.
        assert!(out.contains("WILDCARD_TAG"));
        assert!(out.contains("msg_payload"));
        assert!(out.contains("CounterMsg::PoisonPill(_)"));
        // Stub action closure and target decision.
        assert!(out.contains("let _stub = \"note_unhandled\""));
        assert!(out.contains("Decision::Reset"));
    }

    #[test]
    fn concrete_generation_emits_real_actions_in_root_rules() {
        let config = parse(COUNTER_ROOT_TOML);
        let out = crate::system_spec::generate_concrete_spec_skeleton(
            &config,
            None,
            "counter-blox",
            "crate",
            None,
        )
        .expect("concrete generation must succeed");
        assert!(out.contains("fn root_transitions()"));
        assert!(out.contains("ROOT_RULES"));
        // The Self:: action is resolved to the context-crate function —
        // no stub markers anywhere in a concrete spec.
        assert!(out.contains("blox_ctx_ticks"));
        assert!(out.contains("note_unhandled"));
        assert!(!out.contains("_stub"));
    }

    /// Feature-gated blox exercising `feature_on_init` (mirrors the shape of
    /// crates/bloxes/pool/blox.toml): the feature variant resets the
    /// feature-gated state fields, the base variant must not see them.
    const FEATURE_ON_INIT_TOML: &str = r#"
[actor]
name = "Pool"

[event]
name = "PoolEvent"
generics = "<R: BloxRuntime>"
feature = "dynamic"
feature_generics = "<R: BloxRuntime>"

[[event.mailboxes]]
variant = "Msg"
message = "PoolMsg"
message_path = "pool_messages::PoolMsg"

[context]
name = "PoolCtx"
generics = "<R: BloxRuntime>"
feature = "dynamic"
feature_generics = "<R: BloxRuntime>"
on_init = "ctx.pending = 0;"
feature_on_init = "ctx.pending = 0; ctx.spawn_queue.clear();"

[[context.fields]]
name = "pending"
type = "u32"

[[context.uses]]
feature = "dynamic"
fields = [
    { name = "spawn_queue", ty = "Vec<u32>", role = "state" },
]

[topology]

[[topology.states]]
name = "Idle"
initial = true
"#;

    #[test]
    fn feature_variant_uses_feature_on_init() {
        let out = generate_stub(&parse(FEATURE_ON_INIT_TOML));
        // Both variants reset the base field...
        assert_eq!(
            out.matches("ctx.pending = 0;").count(),
            2,
            "base reset must appear in both variants:\n{out}"
        );
        // ...but the feature-gated field reset appears exactly once, in the
        // feature variant — after its #[cfg(feature = "dynamic")] gate.
        assert_eq!(out.matches("ctx.spawn_queue.clear();").count(), 1);
        let feat_gate = out
            .find("#[cfg(feature = \"dynamic\")]")
            .expect("feature variant must be emitted");
        let extra = out
            .find("ctx.spawn_queue.clear();")
            .expect("feature_on_init body must be emitted");
        assert!(
            feat_gate < extra,
            "feature_on_init body must live in the feature variant:\n{out}"
        );
    }

    /// Feature-gated blox with transitions gated on the enclosing variant's
    /// feature, on a different feature, and on none — exercises the rule-level
    /// #[cfg] dedup inside the already-gated variant impl block.
    const FEATURE_RULE_CFG_TOML: &str = r#"
[actor]
name = "Gate"

[event]
name = "GateEvent"
generics = "<R: BloxRuntime>"
feature = "dynamic"
feature_generics = "<R: BloxRuntime>"

[[event.mailboxes]]
variant = "Msg"
message = "GateMsg"
message_path = "gate_messages::GateMsg"

[context]
name = "GateCtx"
generics = "<R: BloxRuntime>"
feature = "dynamic"
feature_generics = "<R: BloxRuntime>"

[topology]

[[topology.states]]
name = "Idle"
initial = true

[[topology.states]]
name = "Working"

[[topology.transitions]]
state = "Idle"
event = "GateMsg::Go(_)"
target = "Working"
actions = ["Self::go"]

[[topology.transitions]]
state = "Idle"
event = "GateMsg::Spawn(_)"
target = "Working"
actions = ["Self::spawn"]
feature = "dynamic"

[[topology.transitions]]
state = "Idle"
event = "GateMsg::Extra(_)"
target = "Working"
actions = ["Self::extra"]
feature = "extra"
"#;

    #[test]
    fn rule_cfg_duplicating_variant_gate_is_not_emitted() {
        let out = generate_stub(&parse(FEATURE_RULE_CFG_TOML));
        // The feature variant's items carry the block-level gate (struct,
        // state-fns impl, MachineSpec impl) — exactly 3 occurrences. The
        // dynamic-gated rule inside that variant must NOT add its own.
        assert_eq!(
            out.matches("#[cfg(feature = \"dynamic\")]").count(),
            3,
            "rule-level gates duplicating the variant gate must be dropped:\n{out}"
        );
        // A rule gated on a DIFFERENT feature keeps its own gate.
        assert_eq!(
            out.matches("#[cfg(feature = \"extra\")]").count(),
            1,
            "a rule gate differing from the variant gate must be preserved:\n{out}"
        );
        // The dynamic-gated rule itself is still emitted (ungated) inside the
        // feature variant.
        assert!(out.contains("let _stub = \"spawn\""));
    }

    #[test]
    fn system_level_emits_no_rule_cfgs() {
        let out = crate::system_spec::generate_concrete_spec_skeleton(
            &parse(FEATURE_RULE_CFG_TOML),
            None,
            "gate-blox",
            "crate",
            Some("dynamic"),
        )
        .expect("concrete generation must succeed");
        // System-level codegen selects the feature via Cargo.toml and strips
        // every cfg gate, block-level and rule-level alike.
        assert!(
            !out.contains("#[cfg(feature"),
            "system-level specs must carry no feature gates:\n{out}"
        );
    }
}
