// Copyright 2025 Bloxide, all rights reserved
//! Root application component, workspace scanner, heatmap grid, and side panel.
use crate::context_view::{ContextView, SupervisionTreeView};
use crate::diagram::{RawTomlView, StateDiagram};
use crate::editor::EditorPanel;
use crate::model::*;
use crate::server::{scan_workspace, viz_remove_state, viz_remove_transition};
use crate::system_view::SystemView;
use dioxus::prelude::*;
use std::collections::HashSet;

#[derive(Clone, PartialEq)]
pub(crate) enum ViewMode {
    Heatmap,
    Diagram,
    System,
    Supervision,
    Context,
    RawToml,
}

impl ViewMode {
    fn label(&self) -> &'static str {
        match self {
            ViewMode::Heatmap => "Heatmap",
            ViewMode::Diagram => "State Diagram",
            ViewMode::System => "System View",
            ViewMode::Supervision => "Supervision",
            ViewMode::Context => "Context",
            ViewMode::RawToml => "Raw TOML",
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) enum DiagramSelection {
    State(String),
    Transition { state: String, event: String },
}

#[component]
pub(crate) fn App() -> Element {
    // Specs start empty; the user must explicitly load a workspace or import
    // a JSON file. No auto-scan of CARGO_MANIFEST_DIR or current directory.
    let mut specs = use_signal(Vec::<BloxSpec>::new);
    let mut selected_spec = use_signal(|| 0usize);
    let mut selected_cell = use_signal(|| None::<(String, String)>);
    let mut view_mode = use_signal(|| ViewMode::Heatmap);
    let mut selected_diagram = use_signal(|| None::<DiagramSelection>);
    let collapsed_composites = use_signal(HashSet::<String>::new);

    if specs.read().is_empty() {
        return rsx! {
            div {
                style: "font-family: system-ui, -apple-system, sans-serif; padding: 40px; background: #f5f5f5; min-height: 100vh;",
                h1 { style: "margin: 0 0 24px 0; color: #333;", "Bloxide Visualizer" }
                div {
                    style: "display: flex; flex-direction: column; gap: 16px; max-width: 600px;",
                    WorkspaceScanner {
                        specs: specs,
                        selected_spec: selected_spec,
                        selected_cell: selected_cell,
                        selected_diagram: selected_diagram,
                    }
                    label {
                        style: "padding: 8px 16px; background: #10b981; color: white; border: none; border-radius: 4px; cursor: pointer; display: inline-block; font-size: 14px; font-family: system-ui, sans-serif; width: fit-content;",
                        "Import .json"
                        input {
                            r#type: "file",
                            accept: ".json",
                            style: "display: none;",
                            onchange: move |evt| {
                                async move {
                                    for file in evt.files() {
                                        if let Ok(content) = file.read_string().await {
                                            let name = {
                                                let n = file.name()
                                                    .trim_end_matches(".json")
                                                    .trim_end_matches(".JSON")
                                                    .to_string();
                                                if n.is_empty() { "Imported".to_string() } else { n }
                                            };
                                            let imported = match crate::data::parse_json_spec(&name, &content) {
                                                Ok(spec) => spec,
                                                Err(_e) => continue,
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
            }
        };
    }

    let selected_idx = (*selected_spec.read()).min(specs.read().len() - 1);
    let spec = &specs.read()[selected_idx];

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
                        style: if *selected_spec.read() == idx {
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
                    "Import .json"
                    input {
                        r#type: "file",
                        accept: ".json",
                        style: "display: none;",
                        onchange: move |evt| {
                            async move {
                                for file in evt.files() {
                                    if let Ok(content) = file.read_string().await {
                                        let name = {
                                            let n = file.name()
                                                .trim_end_matches(".json")
                                                .trim_end_matches(".JSON")
                                                .to_string();
                                            if n.is_empty() { "Imported".to_string() } else { n }
                                        };
                                        // JSON-only import (blox.toml/viz-export
                                        // JSON model — the markdown parser is
                                        // gone, issue #119).
                                        let imported = match crate::data::parse_json_spec(&name, &content) {
                                            Ok(spec) => spec,
                                            Err(_e) => continue,
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
                for mode in [ViewMode::Heatmap, ViewMode::Diagram, ViewMode::System, ViewMode::Supervision, ViewMode::Context, ViewMode::RawToml] {
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
                                div {
                                    EditorPanel {
                                        spec: spec.clone(),
                                        specs: specs,
                                    }
                                    StateDiagram {
                                        spec: spec.clone(),
                                        selected_diagram: selected_diagram,
                                        collapsed_composites: collapsed_composites,
                                    }
                                }
                            },
                            ViewMode::System => rsx! {
                                SystemView {
                                    specs: specs,
                                    selected_spec: selected_spec,
                                    view_mode: view_mode,
                                    selected_cell: selected_cell,
                                    selected_diagram: selected_diagram,
                                }
                            },
                            ViewMode::Supervision => rsx! {
                                SupervisionTreeView { spec: spec.clone() }
                            },
                            ViewMode::Context => rsx! {
                                ContextView { spec: spec.clone() }
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
                            specs: specs,
                            state,
                            event: Some(event),
                            on_close: move |_| selected_cell.set(None),
                        }
                    },
                    None => match selected_diagram.read().clone() {
                        Some(DiagramSelection::State(state)) => rsx! {
                            SidePanel {
                                spec: spec.clone(),
                                specs: specs,
                                state,
                                event: None,
                                on_close: move |_| selected_diagram.set(None),
                            }
                        },
                        Some(DiagramSelection::Transition { state, event }) => rsx! {
                            SidePanel {
                                spec: spec.clone(),
                                specs: specs,
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
    // Default the scan path to CARGO_MANIFEST_DIR (set when launched via
    // cargo) or the current working directory — never a hardcoded path (#120).
    let mut workspace_path = use_signal(|| {
        std::env::var("BLOXIDE_VIZ_WORKSPACE")
            .or_else(|_| std::env::var("CARGO_MANIFEST_DIR"))
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            })
    });
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
        std::iter::repeat_n("minmax(100px, max-content)", total_columns).collect::<Vec<_>>().join(" ")
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
    specs: Signal<Vec<BloxSpec>>,
    state: String,
    event: Option<String>,
    on_close: EventHandler<()>,
) -> Element {
    let handler = event
        .as_ref()
        .and_then(|e| spec.handler_for(&state, e))
        .cloned();
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

    // Transition detail panel (#118): full match pattern, feature gate,
    // raw guard expression — all from the exported model.
    let pattern = handler
        .as_ref()
        .map(|h| h.pattern.clone())
        .unwrap_or_default();
    let feature_gate = handler.as_ref().and_then(|h| h.feature.clone());

    let has_guard = handler
        .as_ref()
        .map(|h| h.source != HandlerSource::Dropped)
        .unwrap_or(false);
    let guard_desc = handler.as_ref().map(|h| h.guard.description.clone());
    let guard_raw = handler
        .as_ref()
        .map(|h| h.guard.raw.clone())
        .unwrap_or_default();
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

            // Write-back remove actions (#96): edit the real blox.toml from the UI.
            if let Some(e) = event.clone() {
                {
                    let path = format!("{}/blox.toml", spec.crate_path);
                    let state_for_remove = state.clone();
                    let pattern_for_remove = handler
                        .as_ref()
                        .map(|h| h.pattern.clone())
                        .filter(|p| !p.is_empty())
                        .unwrap_or_else(|| e.clone());
                    rsx! {
                        button {
                            style: "margin-bottom: 16px; padding: 6px 14px; background: #fee2e2; color: #991b1b; border: 1px solid #fca5a5; border-radius: 4px; cursor: pointer; font-size: 13px;",
                            onclick: move |_| {
                                let path = path.clone();
                                let state = state_for_remove.clone();
                                let pattern = pattern_for_remove.clone();
                                async move {
                                    if let Ok(new_specs) = viz_remove_transition(path, state, pattern).await {
                                        specs.set(new_specs);
                                    }
                                    on_close.call(());
                                }
                            },
                            "Remove Transition"
                        }
                    }
                }
            } else {
                {
                    let path = format!("{}/blox.toml", spec.crate_path);
                    let state_for_remove = state.clone();
                    rsx! {
                        button {
                            style: "margin-bottom: 16px; padding: 6px 14px; background: #fee2e2; color: #991b1b; border: 1px solid #fca5a5; border-radius: 4px; cursor: pointer; font-size: 13px;",
                            onclick: move |_| {
                                let path = path.clone();
                                let state = state_for_remove.clone();
                                async move {
                                    if let Ok(new_specs) = viz_remove_state(path, state).await {
                                        specs.set(new_specs);
                                    }
                                    on_close.call(());
                                }
                            },
                            "Remove State"
                        }
                    }
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

            if !pattern.is_empty() {
                div {
                    style: "margin-bottom: 16px;",
                    div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "Event Pattern" }
                    div {
                        style: "padding: 10px 12px; background: #f3f4f6; border-radius: 4px; font-size: 12px; color: #374151; font-family: monospace; white-space: pre-wrap; word-break: break-all;",
                        "{pattern}"
                    }
                }
            }

            if let Some(feat) = feature_gate {
                div {
                    style: "margin-bottom: 16px;",
                    div { style: "font-size: 12px; color: #6b7280; margin-bottom: 4px;", "Feature Gate" }
                    span {
                        style: "padding: 4px 10px; background: #ede9fe; color: #5b21b6; border-radius: 9999px; font-size: 12px; font-family: monospace;",
                        "#[cfg(feature = \"{feat}\")]"
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
                        if !guard_raw.is_empty() {
                            div {
                                style: "margin-bottom: 8px; padding: 10px 12px; background: #fef3c7; border-radius: 4px; font-size: 12px; color: #92400e; font-family: monospace; white-space: pre-wrap; word-break: break-all;",
                                "{guard_raw}"
                            }
                        }
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
