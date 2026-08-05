// Copyright 2025 Bloxide, all rights reserved.
//! Emission of the StateFns associated constants (`X_FNS`, `ROOT_RULES`)
//! inside the spec's `impl` block, from TOML transitions and entry/exit hooks.

use quote::{format_ident, quote};

use crate::schema::{EventConfig, TopologyConfig};
use crate::util::to_snake_case;

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
