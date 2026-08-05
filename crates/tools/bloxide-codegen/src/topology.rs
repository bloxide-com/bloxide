// Copyright 2025 Bloxide, all rights reserved
//! Generate state topology enum and `StateTopology` implementation.
//! When declarative transitions are present in the TOML, also generates
//! complete `StateFns` constants with raw `StateRule { ... }` struct literals.

use quote::{format_ident, quote};
use std::collections::HashMap;

use crate::schema::{TopologyConfig, TransitionConfig, ROOT_STATE_KEYWORD};
use crate::util::{to_snake_case, to_upper_snake_case, HEADER};

/// Capitalize the first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
    }
}

// ── Pattern classification ──────────────────────────────────────────────────
//
// The event pattern string from TOML (e.g. "PingPongMsg::Ping(_)") or
// "SupervisorEvent::Child(ChildLifecycleEvent::Stopped { .. })") is classified
// to determine how the `matches` closure and `event_tag` field are generated.
//
// **Full-event patterns** — match the entire event directly.
//   Examples: "SupervisorEvent::Child(ChildLifecycleEvent::Stopped { .. })", "_"
//   Tag: extracted from the pattern path (e.g. SupervisorEvent::CHILD_TAG)
//   Matches: |__ev| matches!(__ev, <pattern>)
//
// **Msg shorthand** — ident ending with "Msg", matching via msg_payload().
//   Tag: WILDCARD_TAG
//   Matches: |__ev| __ev.msg_payload().is_some_and(|__m| matches!(__m, <pattern>))
//
// **Ctrl shorthand** — ident ending with "Ctrl", matching via ctrl_payload().
//   Tag: WILDCARD_TAG
//   Matches: |__ev| __ev.ctrl_payload().is_some_and(|__m| matches!(__m, <pattern>))

#[derive(Copy, Clone, Debug)]
enum PatternKind {
    FullEvent,
    MsgShorthand,
    CtrlShorthand,
}

/// Classify a pattern string from TOML into the kind of matching it needs.
///
/// Assumes the pattern starts with an identifier (letter or underscore).
/// Patterns starting with `(` (tuple struct) or `{` (struct pattern) are
/// not currently produced by the TOML schema and would misclassify as
/// `FullEvent` — this is acceptable since all valid TOML event patterns
/// begin with an enum/variant name.
fn classify_pattern_str(event: &str) -> PatternKind {
    // Find the first identifier in the pattern string.
    // If it ends with "Msg" → MsgShorthand, "Ctrl" → CtrlShorthand, else FullEvent.
    let trimmed = event.trim();

    // Extract the first identifier from the pattern.
    // An identifier starts with a letter or underscore, followed by
    // alphanumeric or underscore characters.
    let first_ident: String = trimmed
        .chars()
        .skip_while(|c| !c.is_alphabetic() && *c != '_')
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if first_ident.ends_with("Msg") {
        return PatternKind::MsgShorthand;
    }
    if first_ident.ends_with("Ctrl") {
        return PatternKind::CtrlShorthand;
    }
    PatternKind::FullEvent
}

/// Check if the event pattern string contains a `::` path separator,
/// indicating a variant-specific pattern like `PeerCtrl::AddPeer(...)`
/// vs a catch-all `PeerCtrl(...)`.
fn has_path_separator_str(event: &str) -> bool {
    event.contains("::")
}

/// Strip bindings from a shorthand pattern, replacing `(...)` with `(_)`.
/// E.g. "PingPongMsg::Pong(pong)" → "PingPongMsg::Pong(_)"
///
/// Nested patterns inside `(...)` are replaced with `_` — only the outer
/// variant is matched. For example, `Foo::Bar(Baz(a, b))` → `Foo::Bar(_)`.
/// This is intentional: the `matches!` closure only needs to distinguish
/// the outer variant, not inspect inner fields.
///
/// Handles or-patterns by splitting on top-level `|` (not inside parentheses),
/// stripping each alternative independently, then re-joining with `|`.
/// E.g. "PeerCtrl::AddPeer(_) | PeerCtrl::RemovePeer(_)" → same (already stripped),
/// but "PeerCtrl::AddPeer(x) | PeerCtrl::RemovePeer(y)" → "PeerCtrl::AddPeer(_) | PeerCtrl::RemovePeer(_)".
fn strip_bindings_from_pattern(event: &str) -> String {
    // Split on top-level `|` (not inside parentheses).
    let alternatives = split_top_level_pipe(event);
    let stripped: Vec<String> = alternatives
        .iter()
        .map(|alt| {
            let trimmed = alt.trim();
            if let Some(open) = trimmed.find('(') {
                let before = &trimmed[..open];
                format!("{before}(_)")
            } else {
                trimmed.to_string()
            }
        })
        .collect();
    stripped.join(" | ")
}

/// Split a pattern string on top-level `|` characters, ignoring `|` inside
/// parentheses.  Returns the alternatives as trimmed strings.
fn split_top_level_pipe(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                parts.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim().to_string());
    parts
}

// ── Catch-all elision ────────────────────────────────────────────────────────
//
// A state may declare a catch-all rule for an event variant (e.g.
// `SupervisorEvent::Child(_)`) after specific rules for each message variant
// (e.g. `SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))`).
// The catch-all shares the same event_tag as the specific rules and is
// evaluated after them, so it is unreachable dead code when the specific
// rules already cover every variant of the mailbox's message enum.
//
// Coverage is decidable only when the TOML declares the message enum's full
// variant set (`[[event.mailboxes]]` → `variants = [...]`). When a state's
// earlier rules cover every declared variant, the catch-all is omitted from
// the emitted StateFns; with partial coverage (or no declared variants) it
// is kept.

/// A mailbox whose message enum has its full variant set declared in the TOML.
struct DeclaredMailbox {
    /// Event variant name (e.g. "Child").
    event_variant: String,
    /// Message enum short name (e.g. "ChildLifecycleEvent").
    msg: String,
    /// Declared variant set of the message enum.
    variants: Vec<String>,
}

/// Short (last-segment, generics-stripped) name of a mailbox's message type,
/// e.g. "ChildCtrl" from "bloxide_child_management::control::ChildCtrl<R>".
fn msg_short_name(mb: &crate::schema::MailboxConfig) -> String {
    let path = mb.message_path.as_deref().unwrap_or(&mb.message);
    let base = path.split('<').next().unwrap_or(path).trim();
    base.rsplit("::").next().unwrap_or(base).trim().to_string()
}

/// Find all `Msg::Variant` path occurrences for message type `msg` in a
/// pattern string, returning the referenced variant names.
fn scan_msg_variants(pattern: &str, msg: &str) -> Vec<String> {
    let mut variants = Vec::new();
    let mut start = 0;
    while let Some(found) = pattern[start..].find(msg) {
        let idx = start + found;
        // The character before `msg` must not continue an identifier or path
        // segment (avoids matching inside `SomeChildLifecycleEvent::X`).
        let before_ok = idx == 0 || {
            let c = pattern.as_bytes()[idx - 1];
            !(c.is_ascii_alphanumeric() || c == b'_' || c == b':')
        };
        if before_ok {
            if let Some(rest) = pattern[idx + msg.len()..].strip_prefix("::") {
                let variant: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !variant.is_empty() {
                    variants.push(variant);
                }
            }
        }
        start = idx + msg.len();
    }
    variants
}

/// Analyze one pattern alternative for catch-all elision.
///
/// Returns `(catchall, specifics)`: `catchall` is `Some(mailbox_index)` when
/// the alternative matches every payload of that mailbox's event variant
/// (`Event::Variant(_)` / `Event::Variant(..)` for full-event patterns, a bare
/// `XxxCtrl` for ctrl shorthand); `specifics` lists the message variants the
/// alternative covers.
fn analyze_alternative(
    alt: &str,
    mailboxes: &[DeclaredMailbox],
) -> (Option<usize>, Vec<(usize, String)>) {
    let trimmed = alt.trim();
    match classify_pattern_str(trimmed) {
        PatternKind::FullEvent => {
            // Outer path is everything before the first '(' (or the whole
            // pattern); the event variant is its last segment.
            let (outer, inner) = match trimmed.find('(') {
                Some(open) => (
                    &trimmed[..open],
                    trimmed[open + 1..].rsplit_once(')').map(|(i, _)| i),
                ),
                None => (trimmed, None),
            };
            let variant = outer.rsplit("::").next().unwrap_or(outer).trim();
            let Some(midx) = mailboxes.iter().position(|m| m.event_variant == variant) else {
                return (None, Vec::new());
            };
            match inner {
                // No payload at all (unit variant) or a wildcard payload —
                // both match every event of this variant.
                None => (Some(midx), Vec::new()),
                Some(inner) if matches!(inner.trim(), "_" | "..") => (Some(midx), Vec::new()),
                _ => {
                    let specifics = scan_msg_variants(trimmed, &mailboxes[midx].msg)
                        .into_iter()
                        .map(|v| (midx, v))
                        .collect();
                    (None, specifics)
                }
            }
        }
        PatternKind::MsgShorthand | PatternKind::CtrlShorthand => {
            // First path token names the message type: a bare `XxxCtrl` /
            // `XxxMsg` is a catch-all, `XxxCtrl::Variant(_)` is specific.
            let first: String = trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let Some(midx) = mailboxes.iter().position(|m| m.msg == first) else {
                return (None, Vec::new());
            };
            if !has_path_separator_str(trimmed) {
                return (Some(midx), Vec::new());
            }
            let variant: String = trimmed[first.len()..]
                .strip_prefix("::")
                .unwrap_or("")
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if variant.is_empty() {
                (None, Vec::new())
            } else {
                (None, vec![(midx, variant)])
            }
        }
    }
}

/// Compute, for each rule in `rules` (evaluated in order), whether the rule is
/// a catch-all made unreachable by EARLIER rules that together cover every
/// declared variant of the same mailbox. Returns one bool per rule — `true`
/// marks the rule for omission from the emitted StateFns.
///
/// Only "pure" catch-all rules are considered: every top-level alternative of
/// the rule's pattern must be a payload wildcard for one declared mailbox.
/// Coverage is positional — a catch-all is elided only when the rules before
/// it provide full coverage — so reordered or partial rule sets keep their
/// catch-alls. Catch-alls themselves contribute no coverage (a second
/// catch-all is not made redundant by a first one that is being kept).
pub(crate) fn catchall_elision(
    rules: &[&TransitionConfig],
    event: Option<&crate::schema::EventConfig>,
) -> Vec<bool> {
    let mailboxes: Vec<DeclaredMailbox> = event
        .map(|ev| {
            ev.mailboxes
                .iter()
                .filter(|mb| !mb.variants.is_empty())
                .map(|mb| DeclaredMailbox {
                    event_variant: mb.variant.clone(),
                    msg: msg_short_name(mb),
                    variants: mb.variants.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    let mut elide = Vec::with_capacity(rules.len());
    if mailboxes.is_empty() {
        elide.resize(rules.len(), false);
        return elide;
    }

    // Message variants covered by specific rules seen so far, per mailbox.
    let mut covered: Vec<std::collections::BTreeSet<String>> =
        mailboxes.iter().map(|_| Default::default()).collect();

    for rule in rules {
        let alternatives = split_top_level_pipe(&rule.event);
        let mut pure_catchall = !alternatives.is_empty();
        let mut contributions: Vec<(usize, String)> = Vec::new();
        for alt in &alternatives {
            let (catchall, specifics) = analyze_alternative(alt, &mailboxes);
            match catchall {
                Some(midx)
                    if mailboxes[midx]
                        .variants
                        .iter()
                        .all(|v| covered[midx].contains(v)) => {}
                _ => pure_catchall = false,
            }
            contributions.extend(specifics);
        }
        elide.push(pure_catchall);
        for (midx, variant) in contributions {
            covered[midx].insert(variant);
        }
    }
    elide
}

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

    Ok(format!("{}{}", HEADER, formatted))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::schema::{EventConfig, TopologyConfig, TransitionConfig};

    fn parse_topology(toml_str: &str) -> TopologyConfig {
        toml::from_str(toml_str).expect("test topology must parse")
    }

    // ── catch-all elision ─────────────────────────────────────────────────

    const ELISION_EVENT_TOML: &str = r#"
name = "SupervisorEvent"

[[mailboxes]]
variant = "Child"
message = "ChildLifecycleEvent"
message_path = "bloxide_core::lifecycle::ChildLifecycleEvent"
variants = ["Started", "Stopped"]

[[mailboxes]]
variant = "Control"
message = "ChildCtrl"
message_path = "bloxide_child_management::control::ChildCtrl<R>"
variants = ["WatchdogTick", "RegisterChild"]
"#;

    fn elision_flags(transitions_toml: &str, event_toml: &str) -> Vec<bool> {
        let topology = parse_topology(&format!(
            "[[states]]\nname = \"Running\"\ninitial = true\n{transitions_toml}"
        ));
        let event: EventConfig = toml::from_str(event_toml).expect("test event must parse");
        let rules: Vec<&TransitionConfig> = topology.transitions.iter().collect();
        super::catchall_elision(&rules, Some(&event))
    }

    #[test]
    fn fully_covered_catchall_is_elided() {
        let flags = elision_flags(
            r#"
[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Started { .. }))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(_)"
target = "stay"
"#,
            ELISION_EVENT_TOML,
        );
        assert_eq!(flags, vec![false, false, true]);
    }

    #[test]
    fn partially_covered_catchall_is_kept() {
        // Only Started has a specific rule; Stopped is uncovered.
        let flags = elision_flags(
            r#"
[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Started { .. }))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(_)"
target = "stay"
"#,
            ELISION_EVENT_TOML,
        );
        assert_eq!(flags, vec![false, false]);
    }

    #[test]
    fn catchall_without_declared_variants_is_kept() {
        let event_toml = r#"
name = "SupervisorEvent"

[[mailboxes]]
variant = "Child"
message = "ChildLifecycleEvent"
"#;
        let flags = elision_flags(
            r#"
[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(_)"
target = "stay"
"#,
            event_toml,
        );
        assert_eq!(flags, vec![false]);
    }

    #[test]
    fn catchall_before_specific_rules_is_kept() {
        // Coverage is positional: the catch-all precedes the specific rules,
        // so it is the one that would match — it must stay.
        let flags = elision_flags(
            r#"
[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(_)"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Started { .. }))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))"
target = "stay"
"#,
            ELISION_EVENT_TOML,
        );
        assert_eq!(flags, vec![false, false, false]);
    }

    #[test]
    fn or_pattern_coverage_counts_per_alternative() {
        // One rule covers Started, an or-pattern covers Stopped + WatchdogTick...
        // The Control catch-all still lacks RegisterChild → kept; the Child
        // catch-all is fully covered → elided.
        let flags = elision_flags(
            r#"
[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Started { .. })) | SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(_)"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Control(Envelope(_, ChildCtrl::WatchdogTick))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Control(_)"
target = "stay"
"#,
            ELISION_EVENT_TOML,
        );
        assert_eq!(flags, vec![false, true, false, false]);
    }

    #[test]
    fn kept_catchall_does_not_elide_a_second_catchall() {
        // No specific coverage at all: two catch-alls for the same tag are
        // both kept (a kept catch-all never provides coverage).
        let flags = elision_flags(
            r#"
[[transitions]]
state = "Running"
event = "SupervisorEvent::Control(_)"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Control(_)"
target = "stay"
"#,
            ELISION_EVENT_TOML,
        );
        assert_eq!(flags, vec![false, false]);
    }

    #[test]
    fn mixed_or_pattern_catchall_is_kept() {
        // One alternative is a catch-all for a fully covered mailbox, the
        // other for a partially covered one — the rule is not a pure
        // fully-covered catch-all, so it stays.
        let flags = elision_flags(
            r#"
[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Started { .. }))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Child(_) | SupervisorEvent::Control(_)"
target = "stay"
"#,
            ELISION_EVENT_TOML,
        );
        assert_eq!(flags, vec![false, false, false]);
    }

    #[test]
    fn fully_covered_control_catchall_is_elided() {
        let flags = elision_flags(
            r#"
[[transitions]]
state = "Running"
event = "SupervisorEvent::Control(Envelope(_, ChildCtrl::WatchdogTick))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Control(Envelope(_, ChildCtrl::RegisterChild(_)))"
target = "stay"

[[transitions]]
state = "Running"
event = "SupervisorEvent::Control(_)"
target = "stay"
"#,
            ELISION_EVENT_TOML,
        );
        assert_eq!(flags, vec![false, false, true]);
    }

    #[test]
    fn bare_ctrl_shorthand_catchall_elided_when_covered() {
        // The mailbox's message type ends with "Ctrl", so patterns classify
        // as ctrl shorthand: a bare `ChildCtrl` is the catch-all.
        let flags = elision_flags(
            r#"
[[transitions]]
state = "Running"
event = "ChildCtrl::WatchdogTick"
target = "stay"

[[transitions]]
state = "Running"
event = "ChildCtrl::RegisterChild(_)"
target = "stay"

[[transitions]]
state = "Running"
event = "ChildCtrl"
target = "stay"
"#,
            ELISION_EVENT_TOML,
        );
        assert_eq!(flags, vec![false, false, true]);
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
