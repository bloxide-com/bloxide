// Copyright 2025 Bloxide, all rights reserved
//! Emission of a single `StateRule { ... }` struct literal from a TOML
//! transition: event tag, `matches` closure, action list, and guard closure.

use quote::{format_ident, quote};

use super::patterns::{
    classify_pattern_str, has_path_separator_str, strip_bindings_from_pattern, PatternKind,
};
use crate::schema::TransitionConfig;
use crate::util::to_upper_snake_case;

/// Extract the event tag expression from a pattern string.
///
/// For full-event patterns like "Enum::Variant(...)":
///   → `Enum::VARIANT_TAG`
/// For wildcard `_`:
///   → `::bloxide_core::event_tag::WILDCARD_TAG`
/// For or-patterns (containing `|`):
///   → `::bloxide_core::event_tag::WILDCARD_TAG`
/// For Msg/Ctrl shorthand:
///   → `::bloxide_core::event_tag::WILDCARD_TAG`
fn extract_event_tag_str(
    event: &str,
    kind: PatternKind,
    type_params: &[String],
) -> proc_macro2::TokenStream {
    // Shorthand patterns always use WILDCARD_TAG
    if !matches!(kind, PatternKind::FullEvent) {
        return quote! { ::bloxide_core::event_tag::WILDCARD_TAG };
    }

    let trimmed = event.trim();

    // Wildcard
    if trimmed == "_" {
        return quote! { ::bloxide_core::event_tag::WILDCARD_TAG };
    }

    // Or-patterns
    if trimmed.contains('|') {
        return quote! { ::bloxide_core::event_tag::WILDCARD_TAG };
    }

    // Try to parse as a path: EnumName::VariantName(...)
    // Extract the path segments before the first `(`
    let path_part = if let Some(open) = trimmed.find('(') {
        &trimmed[..open]
    } else {
        trimmed
    };

    let segments: Vec<&str> = path_part.split("::").map(|s| s.trim()).collect();
    if segments.len() < 2 {
        return quote! { ::bloxide_core::event_tag::WILDCARD_TAG };
    }

    // Last segment is the variant name → convert to UPPER_SNAKE_TAG
    let variant_name = segments.last().unwrap();
    let upper_snake = to_upper_snake_case(variant_name);
    let tag_const = format_ident!("{}_TAG", upper_snake);

    // Build the enum path as a token stream: Segment1 :: Segment2 :: ... :: TAG_CONST
    let enum_segments: Vec<proc_macro2::Ident> = segments[..segments.len() - 1]
        .iter()
        .map(|s| format_ident!("{}", s))
        .collect();

    // Add turbofish for the enum's type parameters if any.
    // E.g. SupervisorEvent::<R>::CHILD_TAG instead of SupervisorEvent::CHILD_TAG
    if type_params.is_empty() {
        quote! { #(#enum_segments ::)* #tag_const }
    } else {
        let param_idents: Vec<proc_macro2::Ident> =
            type_params.iter().map(|s| format_ident!("{}", s)).collect();
        let first = &enum_segments[0];
        let rest = &enum_segments[1..];
        quote! { #first ::<#(#param_idents),*> :: #(#rest ::)* #tag_const }
    }
}

/// Generate the `matches` closure from a pattern string.
fn generate_matches_closure(
    event: &str,
    kind: PatternKind,
) -> anyhow::Result<proc_macro2::TokenStream> {
    let pat_ts: proc_macro2::TokenStream = event
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid event pattern '{}': {}", event, e))?;

    match kind {
        PatternKind::FullEvent => Ok(quote! { |__ev| ::core::matches!(__ev, #pat_ts) }),
        PatternKind::MsgShorthand => {
            let pat_stripped = strip_bindings_from_pattern(event);
            let pat_ts: proc_macro2::TokenStream = pat_stripped
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid msg pattern '{}': {}", pat_stripped, e))?;
            Ok(quote! {
                |__ev| __ev.msg_payload().is_some_and(|__m| ::core::matches!(__m, #pat_ts))
            })
        }
        PatternKind::CtrlShorthand => {
            if has_path_separator_str(event) {
                let pat_stripped = strip_bindings_from_pattern(event);
                let pat_ts: proc_macro2::TokenStream = pat_stripped.parse().map_err(|e| {
                    anyhow::anyhow!("invalid ctrl pattern '{}': {}", pat_stripped, e)
                })?;
                Ok(quote! {
                    |__ev| __ev.ctrl_payload().is_some_and(|__m| ::core::matches!(__m, #pat_ts))
                })
            } else {
                Ok(quote! { |__ev| __ev.ctrl_payload().is_some() })
            }
        }
    }
}

/// Resolve a target string to a Decision expression token stream.
/// "stay" => Decision::Stay, "reset" => Decision::Reset, "stop" => Decision::Stop,
/// "done" => Decision::Done, "fail" => Decision::Fail,
/// "StateName" => Decision::Transition(LeafState::new(StateEnum::StateName))
fn target_to_guard(target: &str, state_enum_ident: &syn::Ident) -> proc_macro2::TokenStream {
    match target {
        "stay" => quote! { ::bloxide_core::transition::Decision::Stay },
        "reset" => quote! { ::bloxide_core::transition::Decision::Reset },
        "stop" => quote! { ::bloxide_core::transition::Decision::Stop },
        "done" => quote! { ::bloxide_core::transition::Decision::Done },
        "fail" => quote! { ::bloxide_core::transition::Decision::Fail },
        state_name => {
            let ident = format_ident!("{}", state_name);
            quote! {
                ::bloxide_core::transition::Decision::Transition(
                    ::bloxide_core::topology::LeafState::new(#state_enum_ident::#ident)
                )
            }
        }
    }
}

/// Generate a guard closure from TOML guards.
///
/// If no guards: returns a simple closure `|_, _, _| Decision::X`.
/// If guards present: generates an if/else-if/else chain.
fn generate_guard_closure(
    trans: &TransitionConfig,
    state_enum_ident: &syn::Ident,
) -> anyhow::Result<proc_macro2::TokenStream> {
    // Separate wildcard guards (condition == "_") from real guards.
    // Wildcard guards provide the fallback target; if present, they override
    // the transition's top-level `target` as the else-branch fallback.
    let mut wildcard_target: Option<proc_macro2::TokenStream> = None;
    let real_guards: Vec<&crate::schema::GuardConfig> = trans
        .guards
        .iter()
        .filter(|g| {
            if g.condition.trim() == "_" {
                wildcard_target = Some(target_to_guard(&g.target, state_enum_ident));
                false
            } else {
                true
            }
        })
        .collect();

    let fallback_target =
        wildcard_target.unwrap_or_else(|| target_to_guard(&trans.target, state_enum_ident));

    if real_guards.is_empty() {
        // No real guards — simple closure
        return Ok(quote! { |ctx, results, _ev| #fallback_target });
    }

    // Build if/else-if/else chain from real guard conditions.
    // Parameters are named so guard conditions can reference ctx and results.
    let mut chain = proc_macro2::TokenStream::new();

    for guard in real_guards {
        let cond_ts: proc_macro2::TokenStream = guard
            .condition
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid guard condition '{}': {}", guard.condition, e))?;
        let guard_target = target_to_guard(&guard.target, state_enum_ident);

        if chain.is_empty() {
            chain = quote! { if #cond_ts { #guard_target } };
        } else {
            chain = quote! { #chain else if #cond_ts { #guard_target } };
        }
    }

    // Add the fallback `else { fallback_target }`
    chain = quote! { #chain else { #fallback_target } };

    Ok(quote! { |ctx, results, _ev| { #chain } })
}

/// Generate a single `StateRule { ... }` struct literal from a TransitionConfig.
///
/// `variant_feature` is the enclosing variant's feature gate (Some(feat) for
/// the feature variant of a paired emission, None otherwise).
// The args are the fixed pieces of emission context threaded through every
// rule; bundling them into a struct would be churn for no real gain.
#[allow(clippy::too_many_arguments)]
pub(crate) fn generate_state_rule(
    trans: &TransitionConfig,
    state_enum_ident: &syn::Ident,
    ctx_type_str: &str,
    event_type_str: &str,
    type_params: &[String],
    action_resolver: crate::ActionResolver<'_>,
    strip_feature_cfg: bool,
    variant_feature: Option<&str>,
) -> anyhow::Result<proc_macro2::TokenStream> {
    let kind = classify_pattern_str(&trans.event);
    let event_tag_ts = extract_event_tag_str(&trans.event, kind, type_params);
    let matches_ts = generate_matches_closure(&trans.event, kind)?;
    let guard_ts = generate_guard_closure(trans, state_enum_ident)?;

    // Resolve action references (with placeholder replacement).
    // `Self::` actions become stub no-op closures; other actions remain
    // function path references (imported via spec_imports).
    let action_tokens: Vec<proc_macro2::TokenStream> = trans
        .actions
        .iter()
        .map(|a| {
            action_resolver(
                a,
                ctx_type_str,
                event_type_str,
                type_params,
                Some(&trans.event),
                true,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let actions_ts = if action_tokens.is_empty() {
        quote! { &[] }
    } else {
        quote! { &[#(#action_tokens),*] }
    };

    let rule = quote! {
        ::bloxide_core::transition::StateRule {
            event_tag: #event_tag_ts,
            matches: #matches_ts,
            actions: #actions_ts,
            guard: #guard_ts,
        }
    };

    // Emit #[cfg(feature = "...")] on the individual StateRule literal,
    // unless strip_feature_cfg is true (system-level codegen where the
    // feature is already selected via Cargo.toml) or the rule's gate
    // duplicates the enclosing variant's #[cfg(feature = "...")] gate.
    Ok(if let Some(ref feat) = trans.feature {
        if strip_feature_cfg || variant_feature == Some(feat.as_str()) {
            rule
        } else {
            quote! {
                #[cfg(feature = #feat)]
                #rule
            }
        }
    } else {
        rule
    })
}
