// Copyright 2025 Bloxide, all rights reserved
mod data;
mod model;
mod parser;

use dioxus::prelude::*;
use dioxus_fullstack::server;
use dioxus_fullstack::ServerFnError;
use model::*;
use std::collections::{HashMap, HashSet};

#[cfg(feature = "server")]
use bloxide_viz_export;

/// Server function: scan a workspace path and return all discovered blox specs.
#[server(endpoint = "api/scan")]
async fn scan_workspace(path: String) -> Result<Vec<BloxSpec>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let workspace = std::path::Path::new(&path);
        if !workspace.exists() {
            return Err(ServerFnError::ServerError {
                message: format!("Path does not exist: {}", path),
                code: 400,
                details: None,
            });
        }
        match bloxide_viz_export::export_workspace(workspace) {
            Ok(specs) => {
                // Convert from bloxide_viz_export::BloxSpec to our model::BloxSpec via JSON
                let json =
                    serde_json::to_string(&specs).map_err(|e| ServerFnError::ServerError {
                        message: format!("JSON serialization failed: {}", e),
                        code: 500,
                        details: None,
                    })?;
                let specs: Vec<BloxSpec> =
                    serde_json::from_str(&json).map_err(|e| ServerFnError::ServerError {
                        message: format!("JSON deserialization failed: {}", e),
                        code: 500,
                        details: None,
                    })?;
                Ok(specs)
            }
            Err(e) => Err(ServerFnError::ServerError {
                message: e,
                code: 500,
                details: None,
            }),
        }
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = path;
        Err(ServerFnError::ServerError {
            message: "Server feature not enabled".to_string(),
            code: 500,
            details: None,
        })
    }
}

fn main() {
    dioxus::launch(App);
}

#[derive(Clone, PartialEq)]
enum ViewMode {
    Heatmap,
    Diagram,
    RawToml,
}

impl ViewMode {
    fn label(&self) -> &'static str {
        match self {
            ViewMode::Heatmap => "Heatmap",
            ViewMode::Diagram => "State Diagram",
            ViewMode::RawToml => "Raw TOML",
        }
    }
}

#[derive(Clone, PartialEq)]
enum DiagramSelection {
    State(String),
    Transition { state: String, event: String },
}

#[component]
fn App() -> Element {
    let mut specs = use_signal(|| data::load_specs());
    let mut selected_spec = use_signal(|| 0usize);
    let mut selected_cell = use_signal(|| None::<(String, String)>);
    let mut view_mode = use_signal(|| ViewMode::Heatmap);
    let mut selected_diagram = use_signal(|| None::<DiagramSelection>);
    let collapsed_composites = use_signal(|| HashSet::<String>::new());

    let spec = &specs.read()[selected_spec.read().clone()];

    let message_sets = spec.message_sets_for_events();
    let leaf_states = spec.leaf_states();

    rsx! {
        div {
            style: "font-family: system-ui, -apple-system, sans-serif; padding: 20px; background: #f5f5f5; min-height: 100vh;",
            h1 { style: "margin: 0 0 20px 0; color: #333;", "Bloxide Visualizer" }
            div {
                style: "display: flex; gap: 10px; margin-bottom: 20px; align-items: center; flex-wrap: wrap;",
                for (idx, s) in specs.read().iter().enumerate() {
                    button {
                        style: if selected_spec.read().clone() == idx {
                            "padding: 8px 16px; background: #2563eb; color: white; border: none; border-radius: 4px; cursor: pointer;"
                        } else {
                            "padding: 8px 16px; background: white; color: #333; border: 1px solid #ddd; border-radius: 4px; cursor: pointer;"
                        },
                        onclick: move |_| {
                            selected_spec.set(idx);
                            selected_cell.set(None);
                            selected_diagram.set(None);
                        },
                        "{s.name}"
                    }
                }
                // Workspace scan input
                WorkspaceScanner {
                    specs: specs,
                    selected_spec: selected_spec,
                    selected_cell: selected_cell,
                    selected_diagram: selected_diagram,
                }
                label {
                    style: "padding: 8px 16px; background: #10b981; color: white; border: none; border-radius: 4px; cursor: pointer; display: inline-block; font-size: 14px; font-family: system-ui, sans-serif;",
                    "Import .md / .json"
                    input {
                        r#type: "file",
                        accept: ".md,.json",
                        style: "display: none;",
                        onchange: move |evt| {
                            async move {
                                for file in evt.files() {
                                    if let Ok(content) = file.read_string().await {
                                        let name = {
                                            let n = file.name()
                                                .trim_end_matches(".md")
                                                .trim_end_matches(".MD")
                                                .trim_end_matches(".json")
                                                .trim_end_matches(".JSON")
                                                .to_string();
                                            if n.is_empty() { "Imported".to_string() } else { n }
                                        };
                                        let imported = if file.name().ends_with(".json") || file.name().ends_with(".JSON") {
                                            match crate::data::parse_json_spec(&name, &content) {
                                                Ok(spec) => spec,
                                                Err(_e) => {
                                                    // On JSON parse failure, fall back to markdown parser
                                                    crate::parser::parse_spec(&name, &content)
                                                }
                                            }
                                        } else {
                                            crate::parser::parse_spec(&name, &content)
                                        };
                                        let new_idx = {
                                            let mut specs_guard = specs.write();
                                            specs_guard.push(imported);
                                            specs_guard.len() - 1
                                        };
                                        selected_cell.set(None);
                                        selected_diagram.set(None);
                                        selected_spec.set(new_idx);
                                    }
                                }
                            }
                        },
                    }
                }
            }
            // View mode toggle
            div {
                style: "display: flex; gap: 0; margin-bottom: 20px;",
                for mode in [ViewMode::Heatmap, ViewMode::Diagram, ViewMode::RawToml] {
                    button {
                        style: if view_mode.read().clone() == mode {
                            "padding: 8px 16px; background: #2563eb; color: white; border: 1px solid #2563eb; cursor: pointer;"
                        } else {
                            "padding: 8px 16px; background: white; color: #333; border: 1px solid #ddd; cursor: pointer;"
                        },
                        onclick: move |_| {
                            view_mode.set(mode.clone());
                            selected_cell.set(None);
                            selected_diagram.set(None);
                        },
                        "{mode.label()}"
                    }
                }
            }
            div {
                style: "display: flex; gap: 20px; justify-content: center; align-items: flex-start;",
                // Main view area
                div {
                    style: "flex: 0 1 auto; max-width: 100%;",
                    h2 { style: "margin: 0 0 10px 0; color: #333; text-align: center;", "{spec.name} {view_mode.read().label()}" }
                    div {
                        style: "background: white; border-radius: 8px; padding: 16px; overflow-x: auto; display: flex; justify-content: center;",
                        match view_mode.read().clone() {
                            ViewMode::Heatmap => rsx! {
                                HeatmapGrid {
                                    spec: spec.clone(),
                                    message_sets,
                                    leaf_states: leaf_states.iter().map(|s| (*s).clone()).collect(),
                                    selected_cell: selected_cell,
                                }
                            },
                            ViewMode::Diagram => rsx! {
                                StateDiagram {
                                    spec: spec.clone(),
                                    selected_diagram: selected_diagram,
                                    collapsed_composites: collapsed_composites,
                                }
                            },
                            ViewMode::RawToml => rsx! {
                                RawTomlView { spec: spec.clone() }
                            },
                        }
                    }
                }
                // Side panel
                match selected_cell.read().clone() {
                    Some((state, event)) => rsx! {
                        SidePanel {
                            spec: spec.clone(),
                            state,
                            event: Some(event),
                            on_close: move |_| selected_cell.set(None),
                        }
                    },
                    None => match selected_diagram.read().clone() {
                        Some(DiagramSelection::State(state)) => rsx! {
                            SidePanel {
                                spec: spec.clone(),
                                state,
                                event: None,
                                on_close: move |_| selected_diagram.set(None),
                            }
                        },
                        Some(DiagramSelection::Transition { state, event }) => rsx! {
                            SidePanel {
                                spec: spec.clone(),
                                state,
                                event: Some(event),
                                on_close: move |_| selected_diagram.set(None),
                            }
                        },
                        None => rsx! {},
                    },
                }
            }
        }
    }
}

#[component]
fn WorkspaceScanner(
    specs: Signal<Vec<BloxSpec>>,
    selected_spec: Signal<usize>,
    selected_cell: Signal<Option<(String, String)>>,
    selected_diagram: Signal<Option<DiagramSelection>>,
) -> Element {
    let mut workspace_path = use_signal(|| "/home/bboganware/repos/bloxide".to_string());
    let mut scan_status = use_signal(|| None::<String>);

    rsx! {
        div {
            style: "display: flex; gap: 8px; align-items: center;",
            input {
                r#type: "text",
                value: "{workspace_path}",
                placeholder: "Path to workspace...",
                style: "padding: 6px 12px; border: 1px solid #d1d5db; border-radius: 4px; font-size: 14px; min-width: 240px;",
                oninput: move |evt| workspace_path.set(evt.value()),
            }
            button {
                style: "padding: 8px 16px; background: #6366f1; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 14px;",
                onclick: move |_| {
                    async move {
                        scan_status.set(Some("Scanning...".to_string()));
                        match scan_workspace(workspace_path()).await {
                            Ok(new_specs) => {
                                let count = new_specs.len();
                                {
                                    let mut specs_guard = specs.write();
                                    for spec in new_specs {
                                        if !specs_guard.iter().any(|s| s.name == spec.name) {
                                            specs_guard.push(spec);
                                        }
                                    }
                                }
                                selected_cell.set(None);
                                selected_diagram.set(None);
                                selected_spec.set(specs.read().len().saturating_sub(count.max(1)));
                                scan_status.set(Some(format!("Found {} blox crate(s)", count)));
                            }
                            Err(e) => {
                                scan_status.set(Some(format!("Error: {}", e)));
                            }
                        }
                    }
                },
                "Scan workspace"
            }
            if let Some(status) = scan_status.read().clone() {
                span {
                    style: "font-size: 12px; color: #6b7280;",
                    "{status}"
                }
            }
        }
    }
}

#[component]
fn HeatmapGrid(
    spec: BloxSpec,
    message_sets: Vec<MessageSet>,
    leaf_states: Vec<State>,
    selected_cell: Signal<Option<(String, String)>>,
) -> Element {
    let total_columns: usize = message_sets.iter().map(|ms| ms.variants.len()).sum();
    let grid_template = format!(
        "display: inline-grid; gap: 1px; background: #e5e7eb; border: 1px solid #d1d5db; border-radius: 4px; overflow: hidden; grid-template-columns: auto {};",
        std::iter::repeat("minmax(100px, max-content)").take(total_columns).collect::<Vec<_>>().join(" ")
    );

    // Build header cells
    let mut header_cells = Vec::new();
    header_cells.push((
        "".to_string(),
        1usize,
        "background: #f9fafb; padding: 8px; font-weight: bold; border-bottom: 1px solid #e5e7eb;"
            .to_string(),
    ));

    for ms in &message_sets {
        let span = ms.variants.len();
        let style = format!(
            "background: #f3f4f6; padding: 8px; font-weight: bold; font-size: 12px; color: #6b7280; text-align: center; border-bottom: 1px solid #e5e7eb; border-right: 1px solid #e5e7eb; grid-column: span {};",
            span
        );
        header_cells.push((ms.name.clone(), span, style));
    }

    // Build sub-header cells
    let mut subheader_cells = Vec::new();
    subheader_cells.push(("State".to_string(), 1usize, "background: #f9fafb; padding: 8px; font-weight: bold; font-size: 12px; border-bottom: 1px solid #e5e7eb;".to_string()));

    for ms in &message_sets {
        for variant in &ms.variants {
            subheader_cells.push((variant.clone(), 1usize, "background: #f9fafb; padding: 8px; font-weight: 600; font-size: 11px; color: #374151; text-align: center; border-bottom: 1px solid #e5e7eb; border-right: 1px solid #e5e7eb;".to_string()));
        }
    }

    // Build data rows
    let mut data_rows = Vec::new();
    for state in &leaf_states {
        let kind_symbol = state.kind.symbol().to_string();
        let state_style = "background: #f9fafb; padding: 6px 12px; font-weight: 600; font-size: 13px; color: #1f2937; border-bottom: 1px solid #e5e7eb; display: flex; align-items: center; white-space: nowrap;".to_string();

        let mut cells = Vec::new();
        for ms in &message_sets {
            for variant in &ms.variants {
                let event_full = format!("{}::{}", ms.name, variant);
                let handler = spec.handler_for(&state.name, &event_full);

                if let Some(h) = handler {
                    let (bg_color, border_style) = match h.source {
                        HandlerSource::Explicit => ("#dbeafe", "2px solid #2563eb"),
                        HandlerSource::Inherited(_) => ("#fef3c7", "2px dashed #f59e0b"),
                        HandlerSource::Dropped => ("#f3f4f6", "1px solid #e5e7eb"),
                    };
                    let is_dropped = h.source == HandlerSource::Dropped;
                    let cell_state = state.name.clone();
                    let cell_event = event_full.clone();
                    let label = h.label.clone();

                    let style = format!(
                        "background: {}; padding: 6px 12px; text-align: center; cursor: pointer; border-bottom: 1px solid #e5e7eb; border-right: 1px solid #e5e7eb; min-height: 32px; display: flex; align-items: center; justify-content: center; border-left: {};",
                        bg_color, border_style
                    );

                    cells.push((cell_state, cell_event, label, is_dropped, style));
                } else {
                    let style = "background: #f3f4f6; padding: 6px 12px; text-align: center; border-bottom: 1px solid #e5e7eb; border-right: 1px solid #e5e7eb; min-height: 32px; display: flex; align-items: center; justify-content: center;".to_string();
                    cells.push((state.name.clone(), event_full, "∅".to_string(), true, style));
                }
            }
        }

        data_rows.push((state.name.clone(), kind_symbol, state_style, cells));
    }

    rsx! {
        div {
            style: "{grid_template}",

            // Header row: message set names
            for (text, _span, style) in header_cells {
                div { style: "{style}", "{text}" }
            }

            // Sub-header row: individual event variants
            for (text, _span, style) in subheader_cells {
                div { style: "{style}", "{text}" }
            }

            // State rows
            for (state_name, kind_symbol, state_style, cells) in data_rows {
                div {
                    style: "{state_style}",
                    span { style: "margin-right: 4px; color: #9ca3af;", "{kind_symbol}" }
                    "{state_name}"
                }

                for (cell_state, cell_event, label, is_dropped, style) in cells {
                    div {
                        style: "{style}",
                        onclick: move |_| selected_cell.set(Some((cell_state.clone(), cell_event.clone()))),
                        if is_dropped {
                            span { style: "color: #9ca3af; font-size: 18px;", "∅" }
                        } else {
                            span { style: "font-size: 11px; color: #1f2937; font-weight: 500;", "{label}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SidePanel(
    spec: BloxSpec,
    state: String,
    event: Option<String>,
    on_close: EventHandler<()>,
) -> Element {
    let handler = event.as_ref().and_then(|e| spec.handler_for(&state, e)).cloned();
    let state_info = spec.state_by_name(&state);
    let entry_exit = spec.entry_exit.get(&state).cloned();

    // Pre-compute all values before RSX
    let state_kind_str = state_info.map(|s| format!("{:?}", s.kind));
    let state_parent = state_info.and_then(|s| s.parent.clone());

    let source_text = handler.as_ref().map(|h| match &h.source {
        HandlerSource::Explicit => "This state's transitions".to_string(),
        HandlerSource::Inherited(parent) => format!("Inherited from {parent}"),
        HandlerSource::Dropped => "No handler — event dropped".to_string(),
    });

    let has_actions = handler
        .as_ref()
        .map(|h| !h.actions.is_empty())
        .unwrap_or(false);
    let actions = handler
        .as_ref()
        .map(|h| h.actions.clone())
        .unwrap_or_default();

    let has_guard = handler
        .as_ref()
        .map(|h| h.source != HandlerSource::Dropped)
        .unwrap_or(false);
    let guard_desc = handler.as_ref().map(|h| h.guard.description.clone());
    let guard_branch_data: Vec<(String, String)> = handler
        .as_ref()
        .map(|h| {
            h.guard
                .branches
                .iter()
                .map(|b| (b.condition.clone(), b.target.display()))
                .collect()
        })
        .unwrap_or_default();

    let target_disp = handler.as_ref().map(|h| h.target.display());

    let title = if let Some(ref e) = event {
        format!("{state} × {e}")
    } else {
        state.clone()
    };

    rsx! {
        div {
            style: "width: 380px; background: white; border-radius: 8px; padding: 20px; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1); max-height: 80vh; overflow-y: auto;",
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                h3 { style: "margin: 0; color: #1f2937; font-size: 16px;", "{title}" }
                button {
                    style: "background: none; border: none; font-size: 20px; cursor: pointer; color: #6b7280;",
                    onclick: move |_| on_close.call(()),
                    "×"
                }
            }

            if let Some(kind_str) = state_kind_str {
                div {
                    style: "margin-bottom: 16px; padding: 12px; background: #f9fafb; border-radius: 6px;",
                    div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "State Kind" }
                    div { style: "font-weight: 600; color: #1f2937;", "{kind_str}" }
                    if let Some(parent) = state_parent {
                        div { style: "margin-top: 8px; font-size: 12px; color: #6b7280;", "Parent: {parent}" }
                    }
                }
            }

            if let Some(source) = source_text {
                div {
                    style: "margin-bottom: 16px;",
                    div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "Declared In" }
                    div {
                        style: "font-weight: 500; color: #1f2937;",
                        "{source}"
                    }
                }
            }

            if has_actions {
                div {
                    style: "margin-bottom: 16px;",
                    div { style: "font-size: 12px; color: #6b7280; margin-bottom: 8px;", "Actions (run before guard)" }
                    for (idx, action) in actions.iter().enumerate() {
                        div {
                            style: "padding: 8px 12px; background: #eff6ff; border-radius: 4px; margin-bottom: 4px; font-size: 13px; color: #1e40af; font-family: monospace;",
                            "{idx + 1}. {action}"
                        }
                    }
                }
            }

            if has_guard {
                if let Some(guard_desc) = guard_desc {
                    div {
                        style: "margin-bottom: 16px;",
                        div { style: "font-size: 12px; color: #6b7280; margin-bottom: 8px;", "Guard (read-only, after actions)" }
                        div {
                            style: "padding: 12px; background: #fefce8; border-radius: 4px; font-size: 13px; color: #713f12; font-family: monospace; white-space: pre-wrap;",
                            "{guard_desc}"
                        }
                        if !guard_branch_data.is_empty() {
                            for (cond, target_disp) in &guard_branch_data {
                                div {
                                    style: "margin-top: 8px; padding: 8px; background: #fffbeb; border-radius: 4px; font-size: 12px;",
                                    span { style: "color: #92400e;", "if {cond} → " }
                                    span { style: "font-weight: 600; color: #92400e;", "{target_disp}" }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(target) = target_disp {
                div {
                    style: "margin-bottom: 16px;",
                    div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "Outcome" }
                    div {
                        style: "font-weight: 600; color: #1f2937;",
                        "{target}"
                    }
                }
            }

            // Entry/Exit
            if let Some(ee) = entry_exit {
                div {
                    style: "border-top: 1px solid #e5e7eb; padding-top: 16px;",
                    if !ee.on_entry.is_empty() {
                        div {
                            style: "margin-bottom: 12px;",
                            div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "on_entry" }
                            for action in &ee.on_entry {
                                div {
                                    style: "padding: 4px 8px; background: #f0fdf4; border-radius: 4px; margin-bottom: 4px; font-size: 12px; color: #166534; font-family: monospace;",
                                    "{action}"
                                }
                            }
                        }
                    }
                    if !ee.on_exit.is_empty() {
                        div {
                            style: "margin-bottom: 12px;",
                            div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "on_exit" }
                            for action in &ee.on_exit {
                                div {
                                    style: "padding: 4px 8px; background: #fdf2f8; border-radius: 4px; margin-bottom: 4px; font-size: 12px; color: #9d174d; font-family: monospace;",
                                    "{action}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// State diagram layout and rendering
// ---------------------------------------------------------------------------

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
        StateKind::Terminal => "#10b981",
        StateKind::Error => "#ef4444",
    }
}

fn state_fill(kind: &StateKind) -> &'static str {
    match kind {
        StateKind::Leaf => "#eff6ff",
        StateKind::Composite => "#faf5ff",
        StateKind::Terminal => "#ecfdf5",
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
        StateKind::Terminal => 2.0,
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
fn StateDiagram(
    spec: BloxSpec,
    selected_diagram: Signal<Option<DiagramSelection>>,
    collapsed_composites: Signal<HashSet<String>>,
) -> Element {
    let layouts = layout_states(&spec, &collapsed_composites.read());

    // Compute diagram bounds
    let (svg_width, svg_height) = layouts.values().fold((0.0_f64, 0.0_f64), |(w, h), node| {
        (w.max(node.x + node.width + 40.0), h.max(node.y + node.height + 40.0))
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
                    transitions.push((handler.clone(), handler.state.clone(), handler.state.clone()));
                }
            }
            Target::Reset => {
                // Reset transitions are not rendered as arrows per issue #69.
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
    let stroke_w = if is_selected { stroke_width + 1.5 } else { stroke_width };

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
fn RawTomlView(spec: BloxSpec) -> Element {
    rsx! {
        div {
            style: "width: 800px; min-height: 400px; padding: 20px; background: #f9fafb; border: 1px solid #e5e7eb; border-radius: 8px; font-family: monospace; font-size: 13px; color: #374151; white-space: pre-wrap;",
            "// Raw TOML source is not currently stored in the BloxSpec model.\n"
            "// The visualizer loads parsed specs from markdown files and JSON exports.\n"
            "// To add raw source viewing, extend the data model to carry the original blox.toml contents.\n\n"
            "Spec: {spec.name}\n"
            "States: {spec.states.len()}\n"
            "Events: {spec.events.len()}\n"
            "Handlers: {spec.handlers.len()}\n"
        }
    }
}
