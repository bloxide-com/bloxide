// Copyright 2025 Bloxide, all rights reserved
//! Catch-all elision: omit catch-all rules made unreachable by earlier rules
//! that already cover every declared variant of the mailbox's message enum.

use super::patterns::{
    classify_pattern_str, has_path_separator_str, split_top_level_pipe, PatternKind,
};
use crate::schema::TransitionConfig;

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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::schema::{EventConfig, TopologyConfig, TransitionConfig};

    fn parse_topology(toml_str: &str) -> TopologyConfig {
        toml::from_str(toml_str).expect("test topology must parse")
    }

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
}
