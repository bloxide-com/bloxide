// Copyright 2025 Bloxide, all rights reserved
//! [`stateDiagram-v2`](https://mermaid.js.org/syntax/stateDiagram.html) text from a snapshot.

use std::collections::BTreeMap;

use crate::snapshot::{
    BloxDiagramSnapshot, StateKindSnapshot, StateSnapshot, TransitionKindSnapshot,
};
use crate::snapshot_hierarchy::{group_children_by_parent, sort_root_states};

/// Render a Mermaid `stateDiagram-v2` document (no leading ``` fence).
pub fn snapshot_to_mermaid(snapshot: &BloxDiagramSnapshot) -> String {
    let mut out = String::new();
    out.push_str("stateDiagram-v2\n");
    out.push_str(&format!("    %% {}\n", escape_mermaid_comment(&snapshot.blox_name)));

    if let Some(target) = snapshot.implicit_entry_target.as_deref() {
        out.push_str(&format!(
            "    [*] --> {} : {}\n",
            mermaid_state_id(target),
            mermaid_edge_label("Start")
        ));
    }

    let mut by_parent = group_children_by_parent(&snapshot.states);
    let mut roots = by_parent.remove(&None).unwrap_or_default();
    sort_root_states(&mut roots, &by_parent);
    for state in roots {
        emit_state_tree(state, &by_parent, &mut out, 1);
    }

    for t in &snapshot.transitions {
        let suffix = match t.transition_kind {
            TransitionKindSnapshot::DomainEvent => "",
            TransitionKindSnapshot::Lifecycle => " · lifecycle",
            TransitionKindSnapshot::RootFallback => " · root",
        };
        let label = if suffix.is_empty() {
            mermaid_edge_label(&t.label)
        } else {
            mermaid_edge_label(&format!("{}{}", t.label, suffix))
        };
        out.push_str(&format!(
            "    {} --> {} : {}\n",
            mermaid_state_id(&t.source_state_id),
            mermaid_state_id(&t.target_state_id),
            label
        ));
    }

    out
}

fn escape_mermaid_comment(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect()
}

/// Mermaid-safe single-line edge label (quoted if needed).
fn mermaid_edge_label(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = if one_line.is_empty() { " ".to_string() } else { one_line };
    let needs_quote = t.contains(':')
        || t.contains('"')
        || t.contains('[')
        || t.contains(']')
        || t.chars().any(|c| c.is_control());
    let escaped = t.replace('"', "'");
    if needs_quote {
        format!("\"{}\"", escaped)
    } else {
        escaped
    }
}

/// State id safe for `stateDiagram-v2` (quoted when not a simple identifier).
fn mermaid_state_id(id: &str) -> String {
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        id.to_string()
    } else {
        format!("\"{}\"", id.replace('"', "'"))
    }
}

fn emit_state_tree(
    state: &StateSnapshot,
    by_parent: &BTreeMap<Option<String>, Vec<&StateSnapshot>>,
    out: &mut String,
    depth: usize,
) {
    let indent = "    ".repeat(depth);
    let key = Some(state.id.clone());
    let children: &[&StateSnapshot] = by_parent.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);

    let is_composite = matches!(state.kind, StateKindSnapshot::Composite) || !children.is_empty();

    if is_composite {
        out.push_str(&format!(
            "{}state {} {{\n",
            indent,
            mermaid_state_id(&state.id)
        ));
        let inner = "    ".repeat(depth + 1);
        if children.is_empty() {
            out.push_str(&format!("{}%% (composite, no children in snapshot)\n", inner));
        } else {
            for child in children {
                emit_state_tree(child, by_parent, out, depth + 1);
            }
        }
        out.push_str(&format!("{}}}\n", indent));
    } else {
        out.push_str(&format!("{}{}\n", indent, mermaid_state_id(&state.id)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::example_ping_blox_snapshot;

    #[test]
    fn ping_example_mermaid_structure() {
        let m = snapshot_to_mermaid(&example_ping_blox_snapshot());
        assert!(m.starts_with("stateDiagram-v2\n"));
        assert!(m.contains("state Operating"));
        assert!(m.contains("[*] --> Active"));
        assert!(m.contains("Active --> Paused"));
        assert!(m.contains("Paused --> Active"));
    }
}
