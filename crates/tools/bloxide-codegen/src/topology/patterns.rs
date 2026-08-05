// Copyright 2025 Bloxide, all rights reserved
//! Event-pattern classification: decide how a TOML event pattern string is
//! matched and how its `event_tag` is derived.

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
pub(super) enum PatternKind {
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
pub(super) fn classify_pattern_str(event: &str) -> PatternKind {
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
pub(super) fn has_path_separator_str(event: &str) -> bool {
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
pub(super) fn strip_bindings_from_pattern(event: &str) -> String {
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
pub(super) fn split_top_level_pipe(s: &str) -> Vec<String> {
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
