// Copyright 2025 Bloxide, all rights reserved.
//! Action-string resolution: replace `{R}`/`{Ctx}`/`{Event}` placeholders and
//! turn action strings into closure or function-path token streams.

use quote::{format_ident, quote, ToTokens};

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
