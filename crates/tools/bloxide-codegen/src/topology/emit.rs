// Copyright 2025 Bloxide, all rights reserved
//! Emission of the state enum, its `StateTopology` implementation, and the
//! `*_handler_table!` macro from a parsed `TopologyConfig`.

use quote::{format_ident, quote};
use std::collections::HashMap;

use crate::schema::{TopologyConfig, ROOT_STATE_KEYWORD};
use crate::util::{to_snake_case, DOC_FROM_BLOX_TOML, HEADER};

/// Capitalize the first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
    }
}

pub fn generate(
    config: &TopologyConfig,
    actor_name: Option<&str>,
    crate_name: &str,
    _has_feature: bool,
) -> anyhow::Result<String> {
    let enum_name_str = actor_name
        .map(|n| format!("{}State", n))
        .unwrap_or_else(|| {
            let base = crate_name.trim_end_matches("-blox");
            let base = base.replace("-", "_");
            let parts: Vec<_> = base.split('_').map(capitalize).collect();
            format!("{}State", parts.join(""))
        });
    let enum_ident = format_ident!("{}", enum_name_str);

    let state_count = config.states.len();

    // Build name -> index map
    let name_to_index: HashMap<String, usize> = config
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.clone(), i))
        .collect();

    // "root" is a reserved keyword: root-level fallback rules are written as
    // state = "root" in [[topology.transitions]], so no user state may take
    // the name.
    for state in &config.states {
        if state.name == ROOT_STATE_KEYWORD {
            anyhow::bail!(
                "state name '{}' is reserved (root-level rules use state = \"root\")",
                ROOT_STATE_KEYWORD
            );
        }
    }

    // Validate parents exist
    for state in &config.states {
        if let Some(ref parent) = state.parent {
            if !name_to_index.contains_key(parent) {
                anyhow::bail!(
                    "state '{}' references unknown parent '{}'",
                    state.name,
                    parent
                );
            }
        }
    }

    // Validate no cycles
    for state in &config.states {
        let mut visited = std::collections::HashSet::new();
        let mut cursor = state.parent.as_ref();
        while let Some(ref cur_name) = cursor {
            if !visited.insert(cur_name.to_string()) {
                anyhow::bail!("cycle detected in parent chain for '{}'", state.name);
            }
            let parent_idx = name_to_index[*cur_name];
            cursor = config.states[parent_idx].parent.as_ref();
        }
    }

    // Validate transition targets reference valid states.
    // state = "root" is the VirtualRoot keyword, not a user-state reference.
    let valid_targets = ["stay", "reset", "stop", "done", "fail"];
    for trans in &config.transitions {
        if trans.state != ROOT_STATE_KEYWORD && !name_to_index.contains_key(&trans.state) {
            anyhow::bail!("transition references unknown state '{}'", trans.state);
        }
        // Validate main target
        let target = &trans.target;
        if !valid_targets.contains(&target.as_str()) && !name_to_index.contains_key(target) {
            anyhow::bail!(
                "transition in state '{}' references unknown target state '{}'",
                trans.state,
                target
            );
        }
        // Validate guard targets
        for guard in &trans.guards {
            let gtarget = &guard.target;
            if !valid_targets.contains(&gtarget.as_str()) && !name_to_index.contains_key(gtarget) {
                anyhow::bail!(
                    "guard in state '{}' references unknown target state '{}'",
                    trans.state,
                    gtarget
                );
            }
        }
    }

    // Validate entry/exit reference valid states
    for ee in config.entry.iter().chain(config.exit.iter()) {
        if !name_to_index.contains_key(&ee.state) {
            anyhow::bail!("entry/exit references unknown state '{}'", ee.state);
        }
    }

    // Enum variants with discriminant
    let enum_variants: Vec<_> = config
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let ident = format_ident!("{}", s.name);
            let idx = i as u8;
            quote! { #ident = #idx }
        })
        .collect();

    let enum_def = quote! {
        #[derive(Copy, Clone, Eq, PartialEq, Debug)]
        #[repr(u8)]
        pub enum #enum_ident {
            #(#enum_variants),*
        }
    };

    // parent() arms
    let parent_arms: Vec<_> = config
        .states
        .iter()
        .map(|s| {
            let ident = format_ident!("{}", s.name);
            match &s.parent {
                None => quote! { Self::#ident => ::core::option::Option::None },
                Some(p) => {
                    let parent_ident = format_ident!("{}", p);
                    quote! { Self::#ident => ::core::option::Option::Some(Self::#parent_ident) }
                }
            }
        })
        .collect();

    // is_leaf() arms — composite states are not leaves
    let is_leaf_arms: Vec<_> = config
        .states
        .iter()
        .map(|s| {
            let ident = format_ident!("{}", s.name);
            let is_leaf = !s.composite.unwrap_or(false);
            quote! { Self::#ident => #is_leaf }
        })
        .collect();

    // Compute paths (root-first, ending at self)
    let paths: Vec<Vec<usize>> = config
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut chain = Vec::new();
            chain.push(i);
            let mut cursor = s.parent.as_ref();
            while let Some(cur_name) = cursor {
                let parent_idx = name_to_index[cur_name];
                chain.push(parent_idx);
                cursor = config.states[parent_idx].parent.as_ref();
            }
            chain.reverse();
            chain
        })
        .collect();

    // path() static arrays and match arms
    let mut path_statics = Vec::new();
    let mut path_arms = Vec::new();

    for (state, path) in config.states.iter().zip(paths.iter()) {
        let vname = format_ident!("{}", state.name);
        let const_name = format_ident!("__PATH_{}", state.name.to_ascii_uppercase());
        let path_idents: Vec<_> = path
            .iter()
            .map(|&idx| {
                let name = format_ident!("{}", config.states[idx].name);
                quote! { #enum_ident::#name }
            })
            .collect();
        let len = path.len();
        path_statics.push(quote! {
            static #const_name: [#enum_ident; #len] = [#(#path_idents),*];
        });
        path_arms.push(quote! {
            Self::#vname => &#const_name
        });
    }

    // as_index() arms
    let as_index_arms: Vec<_> = config
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let ident = format_ident!("{}", s.name);
            quote! { Self::#ident => #i }
        })
        .collect();

    let topology_impl = quote! {
        impl ::bloxide_core::topology::StateTopology for #enum_ident {
            const STATE_COUNT: usize = #state_count;

            #[inline]
            fn parent(self) -> ::core::option::Option<Self> {
                match self {
                    #(#parent_arms,)*
                }
            }

            #[inline]
            fn is_leaf(self) -> bool {
                match self {
                    #(#is_leaf_arms,)*
                }
            }

            fn path(self) -> &'static [Self] {
                #(#path_statics)*
                match self {
                    #(#path_arms,)*
                }
            }

            #[inline]
            fn as_index(self) -> usize {
                match self {
                    #(#as_index_arms,)*
                }
            }
        }
    };

    // The handler_table macro is always generated when there are states.
    // StateFns associated constants are generated by spec_skeleton.rs
    // (inside the MachineSpec impl block). Here we emit the
    // handler_table macro that references those associated constants.

    let fns_idents: Vec<_> = config
        .states
        .iter()
        .map(|s| format_ident!("{}_FNS", to_snake_case(&s.name).to_ascii_uppercase()))
        .collect();

    let macro_name_str = format!("{}_handler_table", to_snake_case(&enum_name_str));
    let macro_name = format_ident!("{}", macro_name_str);

    let handler_code = quote! {
        #[doc(hidden)]
        #[macro_export]
        macro_rules! #macro_name {
            ($ty:ty) => {
                &[#(&<$ty>::#fns_idents),*]
            };
        }
    };

    let tokens = quote! {
        #enum_def
        #topology_impl
        #handler_code
    };

    let raw = tokens.to_string();
    let file = syn::parse_str::<syn::File>(&raw)
        .map_err(|e| anyhow::anyhow!("syn parsing failed: {}", e))?;
    let formatted = prettyplease::unparse(&file);

    Ok(format!("{}{}{}", HEADER, DOC_FROM_BLOX_TOML, formatted))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::schema::TopologyConfig;

    fn parse_topology(toml_str: &str) -> TopologyConfig {
        toml::from_str(toml_str).expect("test topology must parse")
    }

    #[test]
    fn root_transition_unknown_target_errors() {
        let topology = parse_topology(
            r#"
[[states]]
name = "Ready"
initial = true

[[transitions]]
state = "root"
event = "CounterMsg::PoisonPill(_)"
target = "Nowhere"
"#,
        );
        let err = super::generate(&topology, Some("Counter"), "counter-blox", false)
            .expect_err("unknown root target must fail");
        assert!(
            err.to_string()
                .contains("transition in state 'root' references unknown target state 'Nowhere'"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn root_transition_unknown_guard_target_errors() {
        let topology = parse_topology(
            r#"
[[states]]
name = "Ready"
initial = true

[[transitions]]
state = "root"
event = "CounterMsg::PoisonPill(_)"
target = "reset"

[[transitions.guards]]
condition = "ctx.count > 3"
target = "Nowhere"
"#,
        );
        let err = super::generate(&topology, Some("Counter"), "counter-blox", false)
            .expect_err("unknown root guard target must fail");
        assert!(
            err.to_string()
                .contains("guard in state 'root' references unknown target state 'Nowhere'"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn root_transition_valid_targets_accepted() {
        for target in ["stay", "reset", "stop", "done", "fail", "Ready"] {
            let toml_str = format!(
                r#"
[[states]]
name = "Ready"
initial = true

[[transitions]]
state = "root"
event = "CounterMsg::PoisonPill(_)"
target = "{target}"
"#
            );
            let topology = parse_topology(&toml_str);
            super::generate(&topology, Some("Counter"), "counter-blox", false)
                .unwrap_or_else(|e| panic!("target '{target}' must be accepted: {e}"));
        }
    }

    #[test]
    fn state_named_root_is_reserved() {
        let topology = parse_topology(
            r#"
[[states]]
name = "root"
initial = true
"#,
        );
        let err = super::generate(&topology, Some("Counter"), "counter-blox", false)
            .expect_err("a user state named 'root' must fail");
        assert!(
            err.to_string().contains("'root' is reserved"),
            "unexpected error: {err}"
        );
    }
}
