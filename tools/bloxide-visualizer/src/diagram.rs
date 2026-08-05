// Copyright 2025 Bloxide, all rights reserved
//! State diagram layout and rendering.
use crate::app::DiagramSelection;
use crate::model::*;
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

const LEAF_WIDTH: f64 = 160.0;
const LEAF_HEIGHT: f64 = 60.0;
const COMPOSITE_MIN_WIDTH: f64 = 240.0;
const COMPOSITE_HEADER_HEIGHT: f64 = 44.0;
const CHILD_PADDING_X: f64 = 24.0;
const CHILD_PADDING_Y: f64 = 20.0;
const SIBLING_SPACING: f64 = 48.0;
const CHILD_VERTICAL_SPACING: f64 = 28.0;
const SELF_LOOP_RADIUS: f64 = 28.0;

#[derive(Clone, Debug, PartialEq)]
struct LayoutNode {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    state: State,
}

impl LayoutNode {
    fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    fn top_center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y)
    }

    fn bottom_center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height)
    }

    fn right_center(&self) -> (f64, f64) {
        (self.x + self.width, self.y + self.height / 2.0)
    }

    fn left_center(&self) -> (f64, f64) {
        (self.x, self.y + self.height / 2.0)
    }
}

fn layout_states(spec: &BloxSpec, collapsed: &HashSet<String>) -> HashMap<String, LayoutNode> {
    let mut layouts = HashMap::new();
    let root_states: Vec<&State> = spec.states.iter().filter(|s| s.parent.is_none()).collect();

    let mut current_x = 40.0;
    let start_y = 40.0;
    for state in root_states {
        let node = layout_state_recursive(spec, state, current_x, start_y, collapsed, &mut layouts);
        current_x += node.width + SIBLING_SPACING;
    }
    layouts
}

fn layout_state_recursive(
    spec: &BloxSpec,
    state: &State,
    x: f64,
    y: f64,
    collapsed: &HashSet<String>,
    layouts: &mut HashMap<String, LayoutNode>,
) -> LayoutNode {
    if state.kind == StateKind::Composite && !collapsed.contains(&state.name) {
        let children: Vec<&State> = spec
            .states
            .iter()
            .filter(|s| s.parent.as_ref() == Some(&state.name))
            .collect();

        let child_x = x + CHILD_PADDING_X;
        let mut child_y = y + COMPOSITE_HEADER_HEIGHT + CHILD_PADDING_Y;
        let mut max_child_width: f64 = 0.0;
        let mut total_child_height: f64 = 0.0;

        for child in &children {
            let child_node =
                layout_state_recursive(spec, child, child_x, child_y, collapsed, layouts);
            child_y += child_node.height + CHILD_VERTICAL_SPACING;
            max_child_width = max_child_width.max(child_node.width);
            total_child_height += child_node.height + CHILD_VERTICAL_SPACING;
        }

        if !children.is_empty() {
            total_child_height -= CHILD_VERTICAL_SPACING;
        }

        let width = (max_child_width + 2.0 * CHILD_PADDING_X).max(COMPOSITE_MIN_WIDTH);
        let height = if children.is_empty() {
            COMPOSITE_HEADER_HEIGHT + 2.0 * CHILD_PADDING_Y
        } else {
            COMPOSITE_HEADER_HEIGHT + total_child_height + 2.0 * CHILD_PADDING_Y
        };

        let node = LayoutNode {
            x,
            y,
            width,
            height,
            state: state.clone(),
        };
        layouts.insert(state.name.clone(), node.clone());
        node
    } else {
        let node = LayoutNode {
            x,
            y,
            width: LEAF_WIDTH,
            height: LEAF_HEIGHT,
            state: state.clone(),
        };
        layouts.insert(state.name.clone(), node.clone());
        node
    }
}

fn state_color(kind: &StateKind) -> &'static str {
    match kind {
        StateKind::Leaf => "#3b82f6",
        StateKind::Composite => "#8b5cf6",
        StateKind::Error => "#ef4444",
    }
}

fn state_fill(kind: &StateKind) -> &'static str {
    match kind {
        StateKind::Leaf => "#eff6ff",
        StateKind::Composite => "#faf5ff",
        StateKind::Error => "#fef2f2",
    }
}

fn state_stroke_width(kind: &StateKind) -> f64 {
    match kind {
        StateKind::Composite => 2.5,
        _ => 1.5,
    }
}

fn shape_radius(kind: &StateKind) -> f64 {
    match kind {
        StateKind::Leaf => 8.0,
        StateKind::Composite => 10.0,
        StateKind::Error => 2.0,
    }
}

fn arrow_path(source: &LayoutNode, target: &LayoutNode) -> String {
    let (sx, sy) = source.bottom_center();
    let (tx, ty) = target.top_center();

    // If source is above target, use bottom -> top.
    // If they overlap vertically, route from right to left.
    if sy < ty - 10.0 {
        format!("M {sx} {sy} L {tx} {ty}")
    } else if source.x + source.width < target.x {
        let (sx2, sy2) = source.right_center();
        let (tx2, ty2) = target.left_center();
        format!("M {sx2} {sy2} L {tx2} {ty2}")
    } else if target.x + target.width < source.x {
        let (sx2, sy2) = source.left_center();
        let (tx2, ty2) = target.right_center();
        format!("M {sx2} {sy2} L {tx2} {ty2}")
    } else {
        // Fallback: center to center with a slight curve
        let (cx, cy) = source.center();
        let (tcx, tcy) = target.center();
        let mid_x = (cx + tcx) / 2.0;
        let mid_y = (cy + tcy) / 2.0 - 40.0;
        format!("M {cx} {cy} Q {mid_x} {mid_y} {tcx} {tcy}")
    }
}

fn self_loop_path(node: &LayoutNode) -> String {
    let (cx, top_y) = node.top_center();
    let r = SELF_LOOP_RADIUS;
    format!(
        "M {cx} {top_y} C {x1} {y1}, {x2} {y1}, {cx} {top_y}",
        x1 = cx - r * 1.5,
        x2 = cx + r * 1.5,
        y1 = top_y - r * 2.0
    )
}

fn label_for_handler(handler: &Handler) -> String {
    let event_short = handler.event.split("::").last().unwrap_or(&handler.event);
    event_short.to_string()
}

fn guard_label(handler: &Handler) -> Option<String> {
    if handler.guard.description.trim().is_empty() {
        None
    } else {
        Some(handler.guard.description.trim().to_string())
    }
}

#[component]
pub(crate) fn StateDiagram(
    spec: BloxSpec,
    selected_diagram: Signal<Option<DiagramSelection>>,
    collapsed_composites: Signal<HashSet<String>>,
) -> Element {
    let layouts = layout_states(&spec, &collapsed_composites.read());

    // Compute diagram bounds
    let (svg_width, svg_height) = layouts.values().fold((0.0_f64, 0.0_f64), |(w, h), node| {
        (
            w.max(node.x + node.width + 40.0),
            h.max(node.y + node.height + 40.0),
        )
    });

    // Sort states by depth so composite containers render behind children.
    let mut sorted_states: Vec<&State> = spec.states.iter().collect();
    sorted_states.sort_by_key(|s| s.depth);

    // Build explicit transition data.
    let mut transitions: Vec<(Handler, String, String)> = Vec::new();
    for handler in &spec.handlers {
        if handler.source != HandlerSource::Explicit {
            continue;
        }
        match &handler.target {
            Target::Transition(target_name) => {
                if layouts.contains_key(&handler.state) && layouts.contains_key(target_name) {
                    transitions.push((handler.clone(), handler.state.clone(), target_name.clone()));
                }
            }
            Target::Stay => {
                if layouts.contains_key(&handler.state) {
                    transitions.push((
                        handler.clone(),
                        handler.state.clone(),
                        handler.state.clone(),
                    ));
                }
            }
            Target::Reset | Target::Stop | Target::Done | Target::Fail => {
                // Lifecycle outcomes (reset/stop/done/fail) are not rendered
                // as arrows — they are engine-level results, not state targets.
            }
        }
    }

    rsx! {
        svg {
            width: "{svg_width}px",
            height: "{svg_height}px",
            style: "background: #fafafa; border: 1px solid #e5e7eb; border-radius: 8px;",
            xmlns: "http://www.w3.org/2000/svg",

            // Arrow marker definition
            defs {
                marker {
                    id: "arrowhead",
                    marker_width: "10",
                    marker_height: "7",
                    ref_x: "9",
                    ref_y: "3.5",
                    orient: "auto",
                    polygon {
                        points: "0 0, 10 3.5, 0 7",
                        fill: "#6b7280",
                    }
                }
            }

            // Render transitions first so they appear behind nodes.
            for (handler, source_name, target_name) in &transitions {
                TransitionArrow {
                    handler: handler.clone(),
                    source_name: source_name.clone(),
                    target_name: target_name.clone(),
                    layouts: layouts.clone(),
                    selected_diagram: selected_diagram,
                }
            }

            // Render state nodes.
            for state in sorted_states {
                if let Some(node) = layouts.get(&state.name) {
                    StateNode {
                        node: node.clone(),
                        selected_diagram: selected_diagram,
                        collapsed_composites: collapsed_composites,
                    }
                }
            }
        }
    }
}

#[component]
fn TransitionArrow(
    handler: Handler,
    source_name: String,
    target_name: String,
    layouts: HashMap<String, LayoutNode>,
    selected_diagram: Signal<Option<DiagramSelection>>,
) -> Element {
    let source = layouts.get(&source_name).cloned().unwrap();
    let target = layouts.get(&target_name).cloned().unwrap();

    let is_self_loop = source_name == target_name;
    let path_d = if is_self_loop {
        self_loop_path(&source)
    } else {
        arrow_path(&source, &target)
    };

    let event_label = label_for_handler(&handler);
    let guard_text = guard_label(&handler);

    // Label midpoint
    let (label_x, label_y) = if is_self_loop {
        let (cx, top_y) = source.top_center();
        (cx, top_y - SELF_LOOP_RADIUS * 1.6)
    } else {
        let (sx, sy) = source.bottom_center();
        let (tx, ty) = target.top_center();
        ((sx + tx) / 2.0, (sy + ty) / 2.0)
    };

    let is_selected = selected_diagram
        .read()
        .as_ref()
        .map(|sel| matches!(sel, DiagramSelection::Transition { state, event } if *state == handler.state && *event == handler.event))
        .unwrap_or(false);

    let stroke = if is_selected { "#2563eb" } else { "#6b7280" };
    let stroke_width = if is_selected { "2.5" } else { "1.5" };

    rsx! {
        g {
            class: "transition-arrow",
            style: "cursor: pointer;",
            onclick: move |_| {
                selected_diagram.set(Some(DiagramSelection::Transition {
                    state: handler.state.clone(),
                    event: handler.event.clone(),
                }));
            },
            path {
                d: "{path_d}",
                stroke: "{stroke}",
                "stroke-width": "{stroke_width}",
                fill: "none",
                "marker-end": "url(#arrowhead)",
            }
            // Event label background
            rect {
                x: "{label_x - (event_label.len() as f64 * 3.2).min(50.0)}",
                y: "{label_y - 18.0}",
                width: "{(event_label.len() as f64 * 6.4).max(40.0).min(120.0)}",
                height: "16",
                rx: "4",
                fill: "white",
                stroke: "#e5e7eb",
                "stroke-width": "0.5",
            }
            text {
                x: "{label_x}",
                y: "{label_y - 6.0}",
                "text-anchor": "middle",
                "font-size": "11",
                fill: "#374151",
                "font-family": "system-ui, sans-serif",
                "font-weight": "500",
                "pointer-events": "none",
                "{event_label}"
            }
            if let Some(guard) = guard_text {
                text {
                    x: "{label_x}",
                    y: "{label_y + 10.0}",
                    "text-anchor": "middle",
                    "font-size": "10",
                    fill: "#92400e",
                    "font-family": "system-ui, sans-serif",
                    "font-style": "italic",
                    "pointer-events": "none",
                    "[{guard}]"
                }
            }
        }
    }
}

#[component]
fn StateNode(
    node: LayoutNode,
    selected_diagram: Signal<Option<DiagramSelection>>,
    collapsed_composites: Signal<HashSet<String>>,
) -> Element {
    let color = state_color(&node.state.kind);
    let fill = state_fill(&node.state.kind);
    let radius = shape_radius(&node.state.kind);
    let stroke_width = state_stroke_width(&node.state.kind);
    let is_selected = selected_diagram
        .read()
        .as_ref()
        .map(|sel| matches!(sel, DiagramSelection::State(name) if *name == node.state.name))
        .unwrap_or(false);
    let stroke = if is_selected { "#2563eb" } else { color };
    let stroke_w = if is_selected {
        stroke_width + 1.5
    } else {
        stroke_width
    };

    let name = node.state.name.clone();
    let is_composite = node.state.kind == StateKind::Composite;
    let collapsed = collapsed_composites.read().contains(&node.state.name);

    rsx! {
        g {
            style: "cursor: pointer;",
            onclick: move |_| {
                selected_diagram.set(Some(DiagramSelection::State(name.clone())));
            },
            // State shape
            rect {
                x: "{node.x}",
                y: "{node.y}",
                width: "{node.width}",
                height: "{node.height}",
                rx: "{radius}",
                ry: "{radius}",
                fill: "{fill}",
                stroke: "{stroke}",
                "stroke-width": "{stroke_w}",
            }
            // State name label
            text {
                x: "{node.x + node.width / 2.0}",
                y: "{node.y + node.height / 2.0 + 5.0}",
                "text-anchor": "middle",
                "font-size": "13",
                fill: "#1f2937",
                "font-family": "system-ui, sans-serif",
                "font-weight": "600",
                "pointer-events": "none",
                "{node.state.name}"
            }
            // Composite expand/collapse indicator
            if is_composite {
                circle {
                    cx: "{node.x + node.width - 18.0}",
                    cy: "{node.y + 18.0}",
                    r: "10",
                    fill: "white",
                    stroke: "{color}",
                    "stroke-width": "1.5",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        let mut set = collapsed_composites.write();
                        if set.contains(&node.state.name) {
                            set.remove(&node.state.name);
                        } else {
                            set.insert(node.state.name.clone());
                        }
                    },
                }
                text {
                    x: "{node.x + node.width - 18.0}",
                    y: "{node.y + 22.0}",
                    "text-anchor": "middle",
                    "font-size": "12",
                    fill: "{color}",
                    "font-weight": "700",
                    "pointer-events": "none",
                    if collapsed { "+" } else { "−" }
                }
            }
        }
    }
}

#[component]
pub(crate) fn RawTomlView(spec: BloxSpec) -> Element {
    rsx! {
        div {
            style: "width: 800px; min-height: 400px; padding: 20px; background: #f9fafb; border: 1px solid #e5e7eb; border-radius: 8px; font-family: monospace; font-size: 13px; color: #374151; white-space: pre-wrap;",
            "// Raw TOML source is not currently stored in the BloxSpec model.\n"
            "// The visualizer loads parsed specs from JSON exports.\n"
            "// To add raw source viewing, extend the data model to carry the original blox.toml contents.\n\n"
            "Spec: {spec.name}\n"
            "States: {spec.states.len()}\n"
            "Events: {spec.events.len()}\n"
            "Handlers: {spec.handlers.len()}\n"
        }
    }
}
