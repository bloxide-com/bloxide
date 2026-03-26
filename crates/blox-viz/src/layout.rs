// Copyright 2025 Bloxide, all rights reserved
//! Layout: **depth rows** — every leaf at graph distance `d` from the virtual root shares the same
//! `y` (same horizontal band). Composites are sized by [`expand_composite_containers`] around their
//! children. Sibling groups are packed left-to-right; separate root subtrees are separated by a
//! wider gap.
use std::collections::{HashMap, HashSet};

use bloxflow_core::{Edge, Node, PortSide, Position};

use crate::snapshot::{BloxDiagramSnapshot, StateKindSnapshot, StateSnapshot, TransitionKindSnapshot};

const ROW_HEIGHT: f32 = 148.0;
const H_GAP: f32 = 44.0;
const BETWEEN_ROOT_GROUPS: f32 = 56.0;
const LEAF_W: f32 = 176.0;
const LEAF_H: f32 = 68.0;
const COMPOSITE_W: f32 = 240.0;
const COMPOSITE_H: f32 = 76.0;
/// Diameter in graph space for implicit Init (UML **initial pseudostate** — a dot, not a state box).
pub const UML_INIT_DOT_D: f32 = 14.0;
const TOP_MARGIN: f32 = 48.0;

fn node_size(state: &StateSnapshot) -> (f32, f32) {
    match state.kind {
        StateKindSnapshot::Composite => (COMPOSITE_W, COMPOSITE_H),
        StateKindSnapshot::Leaf => (LEAF_W, LEAF_H),
    }
}

/// Tree depth from virtual root (`parent_id == None` → depth 0).
pub fn state_depths(snapshot: &BloxDiagramSnapshot) -> HashMap<String, usize> {
    let mut depth: HashMap<String, usize> = HashMap::new();
    for s in &snapshot.states {
        if s.parent_id.is_none() {
            depth.insert(s.id.clone(), 0);
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for s in &snapshot.states {
            if depth.contains_key(&s.id) {
                continue;
            }
            let Some(p) = &s.parent_id else {
                continue;
            };
            if let Some(&pd) = depth.get(p.as_str()) {
                depth.insert(s.id.clone(), pd + 1);
                changed = true;
            }
        }
    }
    depth
}

/// Topmost ancestor (the state with `parent_id == None` on the path from `id`).
fn root_ancestor(
    id: &str,
    state_by_id: &HashMap<String, &StateSnapshot>,
) -> String {
    let mut cur = id.to_string();
    loop {
        let Some(s) = state_by_id.get(cur.as_str()) else {
            return cur;
        };
        match &s.parent_id {
            None => return cur,
            Some(p) => cur = p.clone(),
        }
    }
}

fn band_y(depth: usize) -> f32 {
    TOP_MARGIN + depth as f32 * ROW_HEIGHT
}

/// Turn a snapshot into Bloxflow geometry (depth-row leaf placement + composite expansion).
pub fn snapshot_to_flow(snapshot: &BloxDiagramSnapshot) -> (Vec<Node>, Vec<Edge>) {
    let mut positions: HashMap<String, Position> = HashMap::new();
    let state_by_id: HashMap<String, &StateSnapshot> =
        snapshot.states.iter().map(|s| (s.id.clone(), s)).collect();

    let mut by_parent: HashMap<Option<String>, Vec<&StateSnapshot>> = HashMap::new();
    for s in &snapshot.states {
        by_parent
            .entry(s.parent_id.clone())
            .or_default()
            .push(s);
    }
    for v in by_parent.values_mut() {
        v.sort_by(|a, b| a.id.cmp(&b.id));
    }
    let by_parent = by_parent;

    let depth = state_depths(snapshot);
    let max_depth = depth.values().copied().max().unwrap_or(0);

    let init_target_depth = snapshot
        .implicit_entry_target
        .as_ref()
        .and_then(|t| depth.get(t.as_str()).copied());

    for d in 0..=max_depth {
        let mut leaves: Vec<&StateSnapshot> = snapshot
            .states
            .iter()
            .filter(|s| {
                matches!(s.kind, StateKindSnapshot::Leaf)
                    && depth.get(s.id.as_str()).copied() == Some(d)
            })
            .collect();

        leaves.sort_by_key(|s| (root_ancestor(s.id.as_str(), &state_by_id), s.id.clone()));

        let mut cursor_x = TOP_MARGIN;
        if init_target_depth == Some(d) {
            cursor_x += UML_INIT_DOT_D + 28.0;
        }

        let mut prev_root: Option<String> = None;
        for s in leaves {
            let r = root_ancestor(s.id.as_str(), &state_by_id);
            if let Some(ref pr) = prev_root {
                if *pr != r {
                    cursor_x += BETWEEN_ROOT_GROUPS;
                }
            }
            prev_root = Some(r);
            let y = band_y(d);
            let w = node_size(s).0;
            positions.insert(s.id.clone(), Position::new(cursor_x, y));
            cursor_x += w + H_GAP;
        }
    }

    let composite_dims = expand_composite_containers(snapshot, &mut positions, &by_parent, &state_by_id);

    separate_depth0_leaves_from_machine(snapshot, &mut positions, &composite_dims, &depth);

    // Implicit Init — left of entry target, vertically centered on that node.
    if snapshot.implicit_entry_target.is_some() {
        let init_id = "__blox_implicit_init";
        if let Some(tid) = snapshot.implicit_entry_target.as_ref() {
            if let Some(tpos) = positions.get(tid.as_str()) {
                let target_state = state_by_id.get(tid.as_str());
                let th = target_state
                    .map(|s| node_size(s).1)
                    .unwrap_or(LEAF_H);
                let gap = 16.0f32;
                let ix = (tpos.x - UML_INIT_DOT_D - gap).max(8.0);
                let iy = tpos.y + th * 0.5 - UML_INIT_DOT_D * 0.5;
                positions.insert(init_id.into(), Position::new(ix, iy));
            } else {
                positions.insert(init_id.into(), Position::new(16.0, band_y(0)));
            }
        }
    }

    let mut nodes: Vec<Node> = snapshot
        .states
        .iter()
        .map(|s| {
            let pos = positions
                .get(&s.id)
                .copied()
                .unwrap_or_else(|| Position::new(0.0, 0.0));
            let (w, h) = composite_dims
                .get(&s.id)
                .copied()
                .unwrap_or_else(|| node_size(s));
            let class = match s.kind {
                StateKindSnapshot::Composite => "bv-composite",
                StateKindSnapshot::Leaf => {
                    if s.parent_id.is_none() {
                        "bv-leaf bv-leaf-root"
                    } else {
                        "bv-leaf"
                    }
                }
            };
            Node::new(s.id.clone(), s.display_name.clone(), pos)
                .with_size(w, h)
                .with_class(class)
        })
        .collect();

    if snapshot.implicit_entry_target.is_some() {
        let init_id = "__blox_implicit_init";
        let pos = positions
            .get(init_id)
            .copied()
            .unwrap_or_else(|| Position::new(16.0, band_y(0)));
        nodes.push(
            Node::new(init_id, "", pos)
                .with_size(UML_INIT_DOT_D, UML_INIT_DOT_D)
                .with_class("bv-init")
                .with_draggable(false),
        );
    }

    let mut edges: Vec<Edge> = Vec::new();

    for s in &snapshot.states {
        if let Some(pid) = &s.parent_id {
            edges.push(
                Edge::new(
                    format!("h_{pid}_{}", s.id),
                    pid.as_str(),
                    s.id.as_str(),
                )
                .with_sides(PortSide::Bottom, PortSide::Top)
                .with_class("bv-edge-hierarchy"),
            );
        }
    }

    if let Some(target) = &snapshot.implicit_entry_target {
        edges.push(
            Edge::new("e_init_start", "__blox_implicit_init", target.as_str())
                .with_label("Start")
                .with_sides(PortSide::Right, PortSide::Left)
                .with_class("bv-edge-start"),
        );
    }

    for t in &snapshot.transitions {
        let edge_class = match t.transition_kind {
            TransitionKindSnapshot::DomainEvent => "bv-edge-domain",
            TransitionKindSnapshot::Lifecycle => "bv-edge-lifecycle",
            TransitionKindSnapshot::RootFallback => "bv-edge-root",
        };
        let mut e = Edge::new(
            t.id.clone(),
            t.source_state_id.clone(),
            t.target_state_id.clone(),
        )
        .with_label(t.label.clone())
        .with_class(edge_class);
        e = match t.transition_kind {
            TransitionKindSnapshot::DomainEvent => e.with_animation(false),
            TransitionKindSnapshot::Lifecycle | TransitionKindSnapshot::RootFallback => {
                e.with_animation(true)
            }
        };
        edges.push(e);
    }

    adjust_edge_ports_from_geometry(&nodes, &mut edges);

    (nodes, edges)
}

/// If root-only depth-0 leaves (e.g. Done / Error) overlap the composite + inner states on the
/// horizontal axis, shift them right of the machine bbox (same band `y`).
fn separate_depth0_leaves_from_machine(
    snapshot: &BloxDiagramSnapshot,
    positions: &mut HashMap<String, Position>,
    composite_dims: &HashMap<String, (f32, f32)>,
    depth: &HashMap<String, usize>,
) {
    let mut machine_min_x = f32::INFINITY;
    let mut machine_max_x = f32::NEG_INFINITY;
    let mut any_machine = false;

    for s in &snapshot.states {
        let Some(&d) = depth.get(s.id.as_str()) else {
            continue;
        };
        // Exclude root-only terminals on the depth-0 band from the silhouette.
        if matches!(s.kind, StateKindSnapshot::Leaf) && s.parent_id.is_none() && d == 0 {
            continue;
        }
        let Some(p) = positions.get(&s.id) else {
            continue;
        };
        let (w, _) = composite_dims
            .get(&s.id)
            .copied()
            .unwrap_or_else(|| node_size(s));
        machine_min_x = machine_min_x.min(p.x);
        machine_max_x = machine_max_x.max(p.x + w);
        any_machine = true;
    }

    if !any_machine || !machine_min_x.is_finite() {
        return;
    }

    let gap = BETWEEN_ROOT_GROUPS;
    for s in &snapshot.states {
        if !matches!(s.kind, StateKindSnapshot::Leaf) {
            continue;
        }
        if s.parent_id.is_none() && depth.get(s.id.as_str()).copied() == Some(0) {
            let Some(pos) = positions.get_mut(&s.id) else {
                continue;
            };
            let w = node_size(s).0;
            if pos.x + w > machine_min_x - gap && pos.x < machine_max_x + gap {
                pos.x = machine_max_x + gap;
            }
        }
    }
}

/// Resize each composite to a **true bounding box** around its children (name compartment + padding)
/// so the solid region reads as a UML **composite state** wrapping its substates.
fn expand_composite_containers(
    snapshot: &BloxDiagramSnapshot,
    positions: &mut HashMap<String, Position>,
    by_parent: &HashMap<Option<String>, Vec<&StateSnapshot>>,
    state_by_id: &HashMap<String, &StateSnapshot>,
) -> HashMap<String, (f32, f32)> {
    const HEADER: f32 = 28.0;
    const PAD: f32 = 14.0;

    let mut dims: HashMap<String, (f32, f32)> = HashMap::new();
    let mut expanded: HashSet<String> = HashSet::new();

    let is_composite = |id: &str| {
        state_by_id
            .get(id)
            .is_some_and(|s| matches!(s.kind, StateKindSnapshot::Composite))
    };

    let mut made_progress = true;
    while made_progress {
        made_progress = false;
        for state in &snapshot.states {
            if !matches!(state.kind, StateKindSnapshot::Composite) {
                continue;
            }
            if expanded.contains(&state.id) {
                continue;
            }
            let key = Some(state.id.clone());
            let Some(children) = by_parent.get(&key) else {
                continue;
            };
            if children.is_empty() {
                expanded.insert(state.id.clone());
                made_progress = true;
                continue;
            }
            if !children.iter().all(|c| {
                !is_composite(c.id.as_str()) || expanded.contains(c.id.as_str())
            }) {
                continue;
            }

            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            let mut any = false;

            for c in children.iter() {
                let Some(p) = positions.get(&c.id) else {
                    continue;
                };
                let (cw, ch) = dims
                    .get(&c.id)
                    .copied()
                    .unwrap_or_else(|| node_size(c));
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x + cw);
                max_y = max_y.max(p.y + ch);
                any = true;
            }

            if !any || !min_x.is_finite() {
                continue;
            }

            let x = min_x - PAD;
            let y = min_y - HEADER - PAD;
            let w = max_x - min_x + 2.0 * PAD;
            let h = max_y - min_y + HEADER + 2.0 * PAD;

            positions.insert(state.id.clone(), Position::new(x, y));
            dims.insert(state.id.clone(), (w, h));
            expanded.insert(state.id.clone());
            made_progress = true;
        }
    }

    dims
}

fn adjust_edge_ports_from_geometry(nodes: &[Node], edges: &mut [Edge]) {
    for e in edges.iter_mut() {
        let Some(s) = nodes.iter().find(|n| n.id == e.source) else {
            continue;
        };
        let Some(t) = nodes.iter().find(|n| n.id == e.target) else {
            continue;
        };
        let (ss, ts) = pick_ports_by_positions(s, t);
        e.source_side = ss;
        e.target_side = ts;
    }
}

fn pick_ports_by_positions(s: &Node, t: &Node) -> (PortSide, PortSide) {
    let scx = s.position.x + s.width * 0.5;
    let scy = s.position.y + s.height * 0.5;
    let tcx = t.position.x + t.width * 0.5;
    let tcy = t.position.y + t.height * 0.5;
    let dx = tcx - scx;
    let dy = tcy - scy;
    if dy.abs() > dx.abs() * 1.05 {
        if dy > 0.0 {
            (PortSide::Bottom, PortSide::Top)
        } else {
            (PortSide::Top, PortSide::Bottom)
        }
    } else if dx > 0.0 {
        (PortSide::Right, PortSide::Left)
    } else {
        (PortSide::Left, PortSide::Right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::example_ping_blox_snapshot;

    #[test]
    fn ping_example_produces_nodes_and_edges() {
        let snap = example_ping_blox_snapshot();
        let (nodes, edges) = snapshot_to_flow(&snap);
        assert!(nodes.len() >= snap.states.len());
        assert!(edges.len() > snap.transitions.len());
        assert!(nodes.iter().any(|n| n.id == "Operating"));
        assert!(edges.iter().any(|e| e.id.starts_with('h')));
    }

    #[test]
    fn ping_leaves_at_same_depth_share_row_y() {
        let snap = example_ping_blox_snapshot();
        let (nodes, _) = snapshot_to_flow(&snap);
        let active = nodes.iter().find(|n| n.id == "Active").expect("Active");
        let paused = nodes.iter().find(|n| n.id == "Paused").expect("Paused");
        assert!(
            (active.position.y - paused.position.y).abs() < 0.5,
            "depth-1 leaves share a band: active.y={} paused.y={}",
            active.position.y,
            paused.position.y
        );
        let done = nodes.iter().find(|n| n.id == "Done").expect("Done");
        let err = nodes.iter().find(|n| n.id == "Error").expect("Error");
        assert!(
            (done.position.y - err.position.y).abs() < 0.5,
            "depth-0 root leaves share a band: done.y={} error.y={}",
            done.position.y,
            err.position.y
        );
        assert!(
            (done.position.y - active.position.y).abs() > 1.0,
            "depth 0 and 1 should be different rows"
        );
    }
}
