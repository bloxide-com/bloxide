// Copyright 2025 Bloxide, all rights reserved
mod data;
mod model;
mod parser;

use dioxus::prelude::*;
use dioxus_fullstack::server;
use dioxus_fullstack::ServerFnError;
use model::*;

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

#[component]
fn App() -> Element {
    let mut specs = use_signal(|| data::load_specs());
    let mut selected_spec = use_signal(|| 0usize);
    let mut selected_cell = use_signal(|| None::<(String, String)>);

    let spec = &specs.read()[selected_spec.read().clone()];

    let message_sets = spec.message_sets_for_events();
    let leaf_states = spec.leaf_states();

    rsx! {
        div {
            style: "font-family: system-ui, -apple-system, sans-serif; padding: 20px; background: #f5f5f5; min-height: 100vh;",
            h1 { style: "margin: 0 0 20px 0; color: #333;", "Bloxide Visualizer" }
            div {
                style: "display: flex; gap: 10px; margin-bottom: 20px; align-items: center;",
                for (idx, s) in specs.read().iter().enumerate() {
                    button {
                        style: if selected_spec.read().clone() == idx {
                            "padding: 8px 16px; background: #2563eb; color: white; border: none; border-radius: 4px; cursor: pointer;"
                        } else {
                            "padding: 8px 16px; background: white; color: #333; border: 1px solid #ddd; border-radius: 4px; cursor: pointer;"
                        },
                        onclick: move |_| selected_spec.set(idx),
                        "{s.name}"
                    }
                }
                // Workspace scan input
                WorkspaceScanner {
                    specs: specs,
                    selected_spec: selected_spec,
                    selected_cell: selected_cell,
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
                                        selected_spec.set(new_idx);
                                    }
                                }
                            }
                        },
                    }
                }
            }
            div {
                style: "display: flex; gap: 20px; justify-content: center; align-items: flex-start;",
                // Main grid
                div {
                    style: "flex: 0 1 auto; max-width: 100%;",
                    h2 { style: "margin: 0 0 10px 0; color: #333; text-align: center;", "{spec.name} Heatmap" }
                    div {
                        style: "background: white; border-radius: 8px; padding: 16px; overflow-x: auto; display: flex; justify-content: center;",
                        HeatmapGrid {
                            spec: spec.clone(),
                            message_sets,
                            leaf_states: leaf_states.iter().map(|s| (*s).clone()).collect(),
                            selected_cell: selected_cell,
                        }
                    }
                }
                // Side panel
                if let Some((state, event)) = selected_cell.read().clone() {
                    SidePanel {
                        spec: spec.clone(),
                        state,
                        event,
                        on_close: move |_| selected_cell.set(None),
                    }
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
fn SidePanel(spec: BloxSpec, state: String, event: String, on_close: EventHandler<()>) -> Element {
    let handler = spec.handler_for(&state, &event).cloned();
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

    rsx! {
        div {
            style: "width: 380px; background: white; border-radius: 8px; padding: 20px; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1); max-height: 80vh; overflow-y: auto;",
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                h3 { style: "margin: 0; color: #1f2937; font-size: 16px;", "{state} × {event}" }
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
