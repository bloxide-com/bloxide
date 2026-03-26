// Copyright 2025 Bloxide, all rights reserved
//! [Graphviz DOT](https://graphviz.org/doc/info/lang.html) for static rasterization (`dot -Tpng`).

use std::collections::BTreeMap;

use crate::snapshot::{
    BloxDiagramSnapshot, StateKindSnapshot, StateSnapshot, TransitionKindSnapshot,
};
use crate::snapshot_hierarchy::{group_children_by_parent, sort_root_states};

/// Directed graph suitable for `dot -Tpng -o out.png graph.dot`.
pub fn snapshot_to_dot(snapshot: &BloxDiagramSnapshot) -> String {
    let graph_name = dot_graph_name(&snapshot.blox_id);
    let mut out = String::new();
    out.push_str(&format!("digraph {} {{\n", graph_name));
    out.push_str("  graph [rankdir=LR, fontname=\"Helvetica\"];\n");
    out.push_str("  node [shape=box, style=rounded, fontname=\"Helvetica\"];\n");
    out.push_str("  edge [fontname=\"Helvetica\", fontsize=10];\n");
    out.push_str(&format!(
        "  label=\"{}\";\n  labelloc=t;\n",
        dot_escape(&snapshot.blox_name)
    ));

    let mut by_parent = group_children_by_parent(&snapshot.states);
    let mut roots = by_parent.remove(&None).unwrap_or_default();
    sort_root_states(&mut roots, &by_parent);

    if snapshot.implicit_entry_target.is_some() {
        out.push_str("  __blox_start [shape=point width=0.06 fixedsize=true label=\"\"];\n");
    }

    for state in roots {
        emit_state_dot(state, &by_parent, &mut out, 1);
    }

    if let Some(target) = snapshot.implicit_entry_target.as_deref() {
        out.push_str(&format!(
            "  __blox_start -> {} [label=\"{}\"];\n",
            dot_node_id(target),
            dot_escape("Start")
        ));
    }

    for t in &snapshot.transitions {
        let suffix = match t.transition_kind {
            TransitionKindSnapshot::DomainEvent => "",
            TransitionKindSnapshot::Lifecycle => " · lifecycle",
            TransitionKindSnapshot::RootFallback => " · root",
        };
        let label = format!("{}{}", t.label, suffix);
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        let label = if label.is_empty() {
            " ".to_string()
        } else {
            label
        };
        out.push_str(&format!(
            "  {} -> {} [label=\"{}\"];\n",
            dot_node_id(&t.source_state_id),
            dot_node_id(&t.target_state_id),
            dot_escape(&label)
        ));
    }

    out.push_str("}\n");
    out
}

fn dot_graph_name(blox_id: &str) -> String {
    let base: String = blox_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = base.trim_matches('_');
    if base.is_empty() || base.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        "BloxDiagram".to_string()
    } else {
        format!("blox_{base}")
    }
}

fn dot_node_id(raw: &str) -> String {
    if raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !raw.is_empty()
        && raw != "graph"
        && raw != "node"
        && raw != "edge"
        && raw != "subgraph"
    {
        raw.to_string()
    } else {
        format!(
            "n_{}",
            raw.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        )
    }
}

fn dot_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

fn emit_state_dot(
    state: &StateSnapshot,
    by_parent: &BTreeMap<Option<String>, Vec<&StateSnapshot>>,
    out: &mut String,
    depth: usize,
) {
    let pad = "  ".repeat(depth);
    let key = Some(state.id.clone());
    let children: &[&StateSnapshot] = by_parent.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);

    let is_composite = matches!(state.kind, StateKindSnapshot::Composite) || !children.is_empty();

    if is_composite {
        let cluster = format!("cluster_{}", dot_node_id(&state.id));
        out.push_str(&format!("{}subgraph {} {{\n", pad, cluster));
        let inner = "  ".repeat(depth + 1);
        out.push_str(&format!(
            "{}label=\"{}\";\n",
            inner,
            dot_escape(&state.display_name)
        ));
        out.push_str(&inner);
        out.push_str("style=dashed;\n");
        out.push_str(&inner);
        out.push_str("color=\"#5a5a5a\";\n");
        if children.is_empty() {
            out.push_str(&inner);
            out.push_str("/* composite, no children in snapshot */\n");
        } else {
            for child in children {
                emit_state_dot(child, by_parent, out, depth + 1);
            }
        }
        out.push_str(&format!("{}}}\n", pad));
    } else {
        let id = dot_node_id(&state.id);
        if state.display_name != state.id {
            out.push_str(&format!(
                "{} {} [label=\"{}\"];\n",
                pad, id, dot_escape(&state.display_name)
            ));
        } else {
            out.push_str(&format!("{} {};\n", pad, id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::example_ping_blox_snapshot;

    #[test]
    fn ping_example_dot_structure() {
        let d = snapshot_to_dot(&example_ping_blox_snapshot());
        assert!(d.starts_with("digraph "));
        assert!(d.contains("subgraph cluster_Operating"));
        assert!(d.contains("__blox_start -> Active"));
        assert!(d.contains("Active -> Paused"));
    }
}
