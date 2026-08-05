// Copyright 2025 Bloxide, all rights reserved
//! System View — actor connection graph (wiring topology).
use crate::app::{DiagramSelection, ViewMode};
use crate::model::*;
use dioxus::prelude::*;
use std::collections::HashMap;

const ACTOR_BLOCK_WIDTH: f64 = 180.0;
const ACTOR_BLOCK_HEIGHT: f64 = 80.0;
const ACTOR_SPACING_X: f64 = 60.0;
const ACTOR_START_X: f64 = 40.0;
const ACTOR_START_Y: f64 = 140.0;
const SUPERVISOR_BLOCK_WIDTH: f64 = 160.0;
const SUPERVISOR_BLOCK_HEIGHT: f64 = 50.0;
const SUPERVISOR_Y: f64 = 40.0;

/// Computed position for an actor in the system view.
#[derive(Clone, Debug, PartialEq)]
struct ActorPosition {
    name: String,
    blox: String,
    x: f64,
    y: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct SupervisorPosition {
    name: String,
    strategy: String,
    x: f64,
    y: f64,
    child_names: Vec<String>,
}

/// State for connection detail panel.
#[derive(Clone, PartialEq)]
struct ConnectionDetail {
    from: String,
    to: String,
    message: String,
    channel_capacity: Option<usize>,
}

/// Layout actors in a horizontal row with equal spacing.
fn layout_actors(wiring: &WiringGraph) -> Vec<ActorPosition> {
    wiring
        .actors
        .iter()
        .enumerate()
        .map(|(idx, actor)| ActorPosition {
            name: actor.name.clone(),
            blox: actor.blox.clone(),
            x: ACTOR_START_X + idx as f64 * (ACTOR_BLOCK_WIDTH + ACTOR_SPACING_X),
            y: ACTOR_START_Y,
        })
        .collect()
}

/// Layout supervisors above their children, centered over the child group.
fn layout_supervisors(
    wiring: &WiringGraph,
    actor_positions: &[ActorPosition],
) -> Vec<SupervisorPosition> {
    wiring
        .supervisors
        .iter()
        .map(|sup| {
            let child_actors: Vec<&ActorPosition> = actor_positions
                .iter()
                .filter(|ap| sup.children.iter().any(|c| c.actor == ap.name))
                .collect();

            let (x, _) = if child_actors.is_empty() {
                (ACTOR_START_X, ACTOR_START_Y)
            } else {
                let min_x = child_actors.iter().map(|a| a.x).fold(f64::MAX, f64::min);
                let max_x = child_actors
                    .iter()
                    .map(|a| a.x + ACTOR_BLOCK_WIDTH)
                    .fold(f64::MIN, f64::max);
                ((min_x + max_x) / 2.0, ACTOR_START_Y)
            };

            SupervisorPosition {
                name: sup.name.clone(),
                strategy: sup.strategy.clone(),
                x: x - SUPERVISOR_BLOCK_WIDTH / 2.0,
                y: SUPERVISOR_Y,
                child_names: sup.children.iter().map(|c| c.actor.clone()).collect(),
            }
        })
        .collect()
}

/// Compute the SVG dimensions needed to fit all actors and supervisors.
fn compute_svg_bounds(
    actor_positions: &[ActorPosition],
    supervisor_positions: &[SupervisorPosition],
) -> (f64, f64) {
    let max_actor_x = actor_positions
        .iter()
        .map(|a| a.x + ACTOR_BLOCK_WIDTH + 40.0)
        .fold(400.0, f64::max);
    let max_actor_y = actor_positions
        .iter()
        .map(|a| a.y + ACTOR_BLOCK_HEIGHT + 40.0)
        .fold(ACTOR_START_Y + ACTOR_BLOCK_HEIGHT + 40.0, f64::max);
    let max_sup_x = supervisor_positions
        .iter()
        .map(|s| s.x + SUPERVISOR_BLOCK_WIDTH + 40.0)
        .fold(max_actor_x, f64::max);
    (max_sup_x, max_actor_y)
}

#[component]
pub(crate) fn SystemView(
    specs: Signal<Vec<BloxSpec>>,
    selected_spec: Signal<usize>,
    view_mode: Signal<ViewMode>,
    selected_cell: Signal<Option<(String, String)>>,
    selected_diagram: Signal<Option<DiagramSelection>>,
) -> Element {
    // Render the wiring of the currently selected system spec (#128 — the
    // spec tabs are the selector across multiple system.toml manifests).
    let wiring_opt: Option<WiringGraph> = specs
        .read()
        .get(*selected_spec.read())
        .and_then(|s| s.wiring.clone());

    let mut selected_connection = use_signal(|| None::<ConnectionDetail>);

    match wiring_opt {
        None => rsx! {
            div {
                style: "padding: 40px; text-align: center; color: #6b7280; font-size: 14px;",
                "This spec has no wiring data. "
                "Select a system spec tab (an app like tokio-demo or tokio-pool-demo) to see its actor connection graph."
            }
        },
        Some(wiring) => {
            let actor_positions = layout_actors(&wiring);
            let supervisor_positions = layout_supervisors(&wiring, &actor_positions);
            let (svg_w, svg_h) = compute_svg_bounds(&actor_positions, &supervisor_positions);

            // Build a name→position lookup for connections.
            let pos_map: HashMap<String, (f64, f64)> = actor_positions
                .iter()
                .map(|ap| (ap.name.clone(), (ap.x, ap.y)))
                .collect();

            // Collect connection data for rendering.
            let conn_data: Vec<(WiringConnection, f64, f64, f64, f64)> = wiring
                .connections
                .iter()
                .filter_map(|conn| {
                    let &(from_x, from_y) = pos_map.get(&conn.from)?;
                    let &(to_x, to_y) = pos_map.get(&conn.to)?;
                    // Arrow from right-center of source to left-center of target.
                    let (sx, sy) = (
                        from_x + ACTOR_BLOCK_WIDTH,
                        from_y + ACTOR_BLOCK_HEIGHT / 2.0,
                    );
                    let (tx, ty) = (to_x, to_y + ACTOR_BLOCK_HEIGHT / 2.0);
                    Some((conn.clone(), sx, sy, tx, ty))
                })
                .collect();

            // Collect supervisor→child lines.
            let sup_lines: Vec<(f64, f64, f64, f64, String, String)> = supervisor_positions
                .iter()
                .flat_map(|sup| {
                    let pm = &pos_map;
                    sup.child_names.iter().filter_map(move |child_name| {
                        let &(child_x, child_y) = pm.get(child_name)?;
                        let (sx, sy) = (
                            sup.x + SUPERVISOR_BLOCK_WIDTH / 2.0,
                            sup.y + SUPERVISOR_BLOCK_HEIGHT,
                        );
                        let (tx, ty) = (child_x + ACTOR_BLOCK_WIDTH / 2.0, child_y);
                        Some((sx, sy, tx, ty, sup.name.clone(), child_name.clone()))
                    })
                })
                .collect();

            rsx! {
                div {
                    style: "display: flex; gap: 20px; align-items: flex-start;",

                    // SVG graph
                    svg {
                        width: "{svg_w}px",
                        height: "{svg_h}px",
                        style: "background: #fafafa; border: 1px solid #e5e7eb; border-radius: 8px;",
                        xmlns: "http://www.w3.org/2000/svg",

                        // Arrow marker definition
                        defs {
                            marker {
                                id: "sys-arrowhead",
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

                        // Supervision tree lines (dashed, behind everything).
                        for (sx, sy, tx, ty, _sup_name, _child_name) in &sup_lines {
                            line {
                                x1: "{sx}",
                                y1: "{sy}",
                                x2: "{tx}",
                                y2: "{ty}",
                                stroke: "#a78bfa",
                                "stroke-width": "1.5",
                                "stroke-dasharray": "6,4",
                                fill: "none",
                            }
                        }

                        // Connection arrows.
                        for (conn, sx, sy, tx, ty) in &conn_data {
                            ConnectionArrow {
                                conn: conn.clone(),
                                sx: *sx,
                                sy: *sy,
                                tx: *tx,
                                ty: *ty,
                                selected_connection: selected_connection,
                            }
                        }

                        // Supervisor nodes.
                        for sup in &supervisor_positions {
                            SupervisorNode {
                                sup: sup.clone(),
                            }
                        }

                        // Actor nodes.
                        for actor in &actor_positions {
                            ActorNode {
                                actor: actor.clone(),
                                specs: specs,
                                selected_spec: selected_spec,
                                view_mode: view_mode,
                                selected_cell: selected_cell,
                                selected_diagram: selected_diagram,
                            }
                        }
                    }

                    // Connection detail panel.
                    if let Some(detail) = selected_connection.read().clone() {
                        ConnectionDetailPanel {
                            detail: detail,
                            on_close: move |_| selected_connection.set(None),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ActorNode(
    actor: ActorPosition,
    specs: Signal<Vec<BloxSpec>>,
    selected_spec: Signal<usize>,
    view_mode: Signal<ViewMode>,
    selected_cell: Signal<Option<(String, String)>>,
    selected_diagram: Signal<Option<DiagramSelection>>,
) -> Element {
    let blox_name = actor.blox.clone();
    let instance_name = actor.name.clone();

    // Find matching spec by blox name for drill-down.
    let mut selected_spec_clone = selected_spec;
    let mut view_mode_clone = view_mode;
    let mut selected_cell_clone = selected_cell;
    let mut selected_diagram_clone = selected_diagram;

    rsx! {
        g {
            style: "cursor: pointer;",
            onclick: move |_| {
                // Drill into state machine view for this blox type.
                let specs_read = specs.read();
                if let Some(idx) = specs_read.iter().position(|s| {
                    s.name.to_lowercase() == blox_name.to_lowercase()
                }) {
                    selected_spec_clone.set(idx);
                    view_mode_clone.set(ViewMode::Heatmap);
                    selected_cell_clone.set(None);
                    selected_diagram_clone.set(None);
                }
            },

            // Actor block background.
            rect {
                x: "{actor.x}",
                y: "{actor.y}",
                width: "{ACTOR_BLOCK_WIDTH}",
                height: "{ACTOR_BLOCK_HEIGHT}",
                rx: "10",
                ry: "10",
                fill: "#eff6ff",
                stroke: "#2563eb",
                "stroke-width": "2",
            }

            // Instance name (title).
            text {
                x: "{actor.x + ACTOR_BLOCK_WIDTH / 2.0}",
                y: "{actor.y + 22.0}",
                "text-anchor": "middle",
                "font-size": "14",
                "font-weight": "700",
                fill: "#1e40af",
                "font-family": "system-ui, sans-serif",
                "pointer-events": "none",
                "{instance_name}"
            }

            // Blox type (subtitle).
            text {
                x: "{actor.x + ACTOR_BLOCK_WIDTH / 2.0}",
                y: "{actor.y + 42.0}",
                "text-anchor": "middle",
                "font-size": "12",
                fill: "#6b7280",
                "font-family": "system-ui, sans-serif",
                "pointer-events": "none",
                "blox: {blox_name}"
            }

        }
    }
}

#[component]
fn SupervisorNode(sup: SupervisorPosition) -> Element {
    let strategy_badge_x = sup.x + SUPERVISOR_BLOCK_WIDTH - 8.0;
    let strategy_badge_y = sup.y + 8.0;

    rsx! {
        g {
            // Supervisor block.
            rect {
                x: "{sup.x}",
                y: "{sup.y}",
                width: "{SUPERVISOR_BLOCK_WIDTH}",
                height: "{SUPERVISOR_BLOCK_HEIGHT}",
                rx: "8",
                ry: "8",
                fill: "#faf5ff",
                stroke: "#8b5cf6",
                "stroke-width": "2",
                "stroke-dasharray": "none",
            }

            // Supervisor name.
            text {
                x: "{sup.x + SUPERVISOR_BLOCK_WIDTH / 2.0}",
                y: "{sup.y + 20.0}",
                "text-anchor": "middle",
                "font-size": "13",
                "font-weight": "600",
                fill: "#6d28d9",
                "font-family": "system-ui, sans-serif",
                "pointer-events": "none",
                "{sup.name}"
            }

            // Strategy badge.
            rect {
                x: "{strategy_badge_x - 70.0}",
                y: "{strategy_badge_y + 18.0}",
                width: "76",
                height: "16",
                rx: "8",
                fill: "#ede9fe",
                stroke: "#8b5cf6",
                "stroke-width": "0.5",
            }
            text {
                x: "{strategy_badge_x - 32.0}",
                y: "{strategy_badge_y + 29.0}",
                "text-anchor": "middle",
                "font-size": "10",
                fill: "#6d28d9",
                "font-family": "system-ui, sans-serif",
                "font-weight": "500",
                "pointer-events": "none",
                "{sup.strategy}"
            }
        }
    }
}

#[component]
fn ConnectionArrow(
    conn: WiringConnection,
    sx: f64,
    sy: f64,
    tx: f64,
    ty: f64,
    selected_connection: Signal<Option<ConnectionDetail>>,
) -> Element {
    let mid_x = (sx + tx) / 2.0;
    let mid_y = (sy + ty) / 2.0;
    let label_text = conn.message.clone();

    let is_selected = selected_connection
        .read()
        .as_ref()
        .map(|d| d.from == conn.from && d.to == conn.to && d.message == conn.message)
        .unwrap_or(false);

    let stroke = if is_selected { "#2563eb" } else { "#6b7280" };
    let stroke_width = if is_selected { "2.5" } else { "1.5" };

    let conn_clone = conn.clone();

    rsx! {
        g {
            style: "cursor: pointer;",
            onclick: move |_| {
                selected_connection.set(Some(ConnectionDetail {
                    from: conn_clone.from.clone(),
                    to: conn_clone.to.clone(),
                    message: conn_clone.message.clone(),
                    channel_capacity: conn_clone.channel_capacity,
                }));
            },

            // Arrow line.
            line {
                x1: "{sx}",
                y1: "{sy}",
                x2: "{tx}",
                y2: "{ty}",
                stroke: "{stroke}",
                "stroke-width": "{stroke_width}",
                "marker-end": "url(#sys-arrowhead)",
            }

            // Label background.
            rect {
                x: "{mid_x - (label_text.len() as f64 * 3.5).min(50.0)}",
                y: "{mid_y - 10.0}",
                width: "{(label_text.len() as f64 * 7.0).max(50.0).min(140.0)}",
                height: "16",
                rx: "4",
                fill: "white",
                stroke: "#e5e7eb",
                "stroke-width": "0.5",
            }

            // Message type label.
            text {
                x: "{mid_x}",
                y: "{mid_y + 2.0}",
                "text-anchor": "middle",
                "font-size": "11",
                fill: "#374151",
                "font-family": "system-ui, sans-serif",
                "font-weight": "500",
                "pointer-events": "none",
                "{label_text}"
            }
        }
    }
}

#[component]
fn ConnectionDetailPanel(detail: ConnectionDetail, on_close: EventHandler<()>) -> Element {
    let capacity_text = match detail.channel_capacity {
        Some(cap) => format!("{}", cap),
        None => "unbounded".to_string(),
    };

    rsx! {
        div {
            style: "width: 320px; background: white; border-radius: 8px; padding: 20px; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1);",
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                h3 { style: "margin: 0; color: #1f2937; font-size: 16px;", "Connection" }
                button {
                    style: "background: none; border: none; font-size: 20px; cursor: pointer; color: #6b7280;",
                    onclick: move |_| on_close.call(()),
                    "×"
                }
            }

            div {
                style: "margin-bottom: 12px; padding: 12px; background: #f9fafb; border-radius: 6px;",
                div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "From" }
                div { style: "font-weight: 600; color: #1e40af;", "{detail.from}" }
            }

            div {
                style: "margin-bottom: 12px; padding: 12px; background: #f9fafb; border-radius: 6px;",
                div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "To" }
                div { style: "font-weight: 600; color: #1e40af;", "{detail.to}" }
            }

            div {
                style: "margin-bottom: 12px; padding: 12px; background: #f9fafb; border-radius: 6px;",
                div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "Message Type" }
                div { style: "font-weight: 600; color: #1f2937; font-family: monospace;", "{detail.message}" }
            }

            div {
                style: "padding: 12px; background: #f9fafb; border-radius: 6px;",
                div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "Channel Capacity" }
                div { style: "font-weight: 600; color: #1f2937;", "{capacity_text}" }
            }
        }
    }
}
