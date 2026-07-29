// Copyright 2025 Bloxide, all rights reserved
mod data;
mod model;

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

// ── Write-back server functions (#96): UI edit → toml_edit mutation on the
// real blox.toml → cargo blox generate → re-export → UI refresh. ──────────

/// Result of an edit operation: fresh specs after write-back + regenerate.
#[cfg(feature = "server")]
fn apply_edit_and_reexport<F>(blox_toml_path: &str, edit: F) -> Result<Vec<BloxSpec>, ServerFnError>
where
    F: FnOnce(&mut toml_edit::DocumentMut) -> anyhow::Result<()>,
{
    let path = std::path::Path::new(blox_toml_path);
    let server_err = |e: String| ServerFnError::ServerError {
        message: e,
        code: 400,
        details: None,
    };

    let mut doc = bloxide_codegen::edit::load_blox_toml(path)
        .map_err(|e| server_err(format!("failed to load {}: {}", blox_toml_path, e)))?;
    edit(&mut doc).map_err(|e| server_err(format!("edit failed: {}", e)))?;
    bloxide_codegen::edit::save_blox_toml(path, &doc)
        .map_err(|e| server_err(format!("failed to save {}: {}", blox_toml_path, e)))?;

    // Regenerate code from the edited manifest (non-fatal — the visual
    // round-trip completes via re-export either way).
    let _ = std::process::Command::new("cargo")
        .args(["blox", "generate"])
        .current_dir(path.parent().unwrap_or(std::path::Path::new(".")))
        .output();

    // Re-export fresh specs from the workspace ROOT (nearest ancestor
    // containing a Cargo.toml with [workspace]) — not the crate dir itself.
    let mut dir = path.parent();
    let mut found = None;
    for _ in 0..8 {
        let Some(d) = dir else { break };
        if let Ok(cargo) = std::fs::read_to_string(d.join("Cargo.toml")) {
            if cargo.contains("[workspace]") {
                found = Some(d.to_path_buf());
                break;
            }
        }
        dir = d.parent();
    }
    let Some(workspace) = found else {
        return Err(server_err("could not locate workspace root".to_string()));
    };
    let specs = bloxide_viz_export::export_workspace(&workspace)
        .map_err(|e| server_err(format!("re-export failed: {}", e)))?;
    let json = serde_json::to_string(&specs).map_err(|e| server_err(e.to_string()))?;
    serde_json::from_str(&json).map_err(|e| server_err(e.to_string()))
}

#[server(endpoint = "api/viz_add_state")]
async fn viz_add_state(
    path: String,
    name: String,
    parent: Option<String>,
    composite: bool,
    error: bool,
) -> Result<Vec<BloxSpec>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        apply_edit_and_reexport(&path, |doc| {
            bloxide_codegen::edit::add_state(doc, &name, parent.as_deref(), composite, error)
        })
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (path, name, parent, composite, error);
        Err(ServerFnError::ServerError {
            message: "Server feature not enabled".to_string(),
            code: 500,
            details: None,
        })
    }
}

#[server(endpoint = "api/viz_remove_state")]
async fn viz_remove_state(path: String, name: String) -> Result<Vec<BloxSpec>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        apply_edit_and_reexport(&path, |doc| {
            bloxide_codegen::edit::remove_state(doc, &name)
        })
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (path, name);
        Err(ServerFnError::ServerError {
            message: "Server feature not enabled".to_string(),
            code: 500,
            details: None,
        })
    }
}

#[server(endpoint = "api/viz_add_transition")]
async fn viz_add_transition(
    path: String,
    state: String,
    event: String,
    target: String,
    actions: Vec<String>,
    guards: Vec<String>,
) -> Result<Vec<BloxSpec>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        apply_edit_and_reexport(&path, |doc| {
            bloxide_codegen::edit::add_transition(
                doc,
                &state,
                &event,
                &target,
                actions.clone(),
                guards.clone(),
                None,
            )
        })
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (path, state, event, target, actions, guards);
        Err(ServerFnError::ServerError {
            message: "Server feature not enabled".to_string(),
            code: 500,
            details: None,
        })
    }
}

#[server(endpoint = "api/viz_remove_transition")]
async fn viz_remove_transition(
    path: String,
    state: String,
    event: String,
) -> Result<Vec<BloxSpec>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        apply_edit_and_reexport(&path, |doc| {
            bloxide_codegen::edit::remove_transition(doc, &state, &event)
        })
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (path, state, event);
        Err(ServerFnError::ServerError {
            message: "Server feature not enabled".to_string(),
            code: 500,
            details: None,
        })
    }
}

/// Server function: export blox specs from the default workspace.
///
/// The default workspace is found by starting at CARGO_MANIFEST_DIR (set by
/// `cargo run`) or the current directory and walking up ancestors until a
/// directory containing blox.toml files is found (issue #119: blox.toml is
/// the sole data source — the markdown parser is gone).
#[server(endpoint = "api/default_specs")]
async fn default_specs() -> Result<Vec<BloxSpec>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let start = std::env::var("BLOXIDE_VIZ_WORKSPACE")
            .or_else(|_| std::env::var("CARGO_MANIFEST_DIR"))
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            });
        let mut dir: Option<&std::path::Path> = Some(std::path::Path::new(&start));
        let mut found = None;
        for _ in 0..6 {
            let Some(d) = dir else { break };
            if bloxide_viz_export::find_blox_tomls(d).first().is_some() {
                found = Some(d.to_path_buf());
                break;
            }
            dir = d.parent();
        }
        let Some(workspace) = found else {
            return Err(ServerFnError::ServerError {
                message: format!("No blox.toml files found at or above {}", start),
                code: 404,
                details: None,
            });
        };
        match bloxide_viz_export::export_workspace(&workspace) {
            Ok(specs) => {
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
        Err(ServerFnError::ServerError {
            message: "Server feature not enabled".to_string(),
            code: 500,
            details: None,
        })
    }
}

#[derive(Clone, PartialEq)]
enum ViewMode {
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
enum DiagramSelection {
    State(String),
    Transition { state: String, event: String },
}

#[component]
fn App() -> Element {
    // Specs load asynchronously from the server (blox.toml is the sole data
    // source — exported via bloxide-viz-export, issue #119).
    let mut specs = use_signal(Vec::<BloxSpec>::new);
    let default_load =
        use_resource(move || async move { default_specs().await.ok().unwrap_or_default() });
    use_effect(move || {
        let loaded = default_load.read().clone();
        if let Some(loaded) = loaded {
            if !loaded.is_empty() && specs.read().is_empty() {
                specs.set(loaded);
            }
        }
    });
    let mut selected_spec = use_signal(|| 0usize);
    let mut selected_cell = use_signal(|| None::<(String, String)>);
    let mut view_mode = use_signal(|| ViewMode::Heatmap);
    let mut selected_diagram = use_signal(|| None::<DiagramSelection>);
    let collapsed_composites = use_signal(|| HashSet::<String>::new());

    if specs.read().is_empty() {
        return rsx! {
            div {
                style: "font-family: system-ui, -apple-system, sans-serif; padding: 20px; background: #f5f5f5; min-height: 100vh;",
                h1 { style: "margin: 0 0 20px 0; color: #333;", "Bloxide Visualizer" }
                p { style: "color: #666;", "Loading workspace specs from blox.toml…" }
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
                                        specs: specs.clone(),
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
                                    specs: specs.clone(),
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
                            specs: specs.clone(),
                            state,
                            event: Some(event),
                            on_close: move |_| selected_cell.set(None),
                        }
                    },
                    None => match selected_diagram.read().clone() {
                        Some(DiagramSelection::State(state)) => rsx! {
                            SidePanel {
                                spec: spec.clone(),
                                specs: specs.clone(),
                                state,
                                event: None,
                                on_close: move |_| selected_diagram.set(None),
                            }
                        },
                        Some(DiagramSelection::Transition { state, event }) => rsx! {
                            SidePanel {
                                spec: spec.clone(),
                                specs: specs.clone(),
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

// ---------------------------------------------------------------------------
// System View — actor connection graph (wiring topology)
// ---------------------------------------------------------------------------

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
fn SystemView(
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
    let mut selected_spec_clone = selected_spec.clone();
    let mut view_mode_clone = view_mode.clone();
    let mut selected_cell_clone = selected_cell.clone();
    let mut selected_diagram_clone = selected_diagram.clone();

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

// ── Context view (#124): context struct, actions, and message definitions ──

#[component]
fn ContextView(spec: BloxSpec) -> Element {
    rsx! {
        div {
            style: "display: flex; gap: 20px; align-items: flex-start; max-width: 1100px;",

            // Context struct
            div {
                style: "min-width: 320px; background: #f9fafb; border-radius: 8px; padding: 16px;",
                h3 { style: "margin: 0 0 12px 0; color: #1f2937; font-size: 15px;", "Context" }
                if let Some(ctx) = &spec.context {
                    div {
                        style: "margin-bottom: 12px; font-family: monospace; font-weight: 600; color: #1e40af;",
                        "pub struct {ctx.struct_name}"
                    }
                    for field in &ctx.fields {
                        div {
                            style: "display: flex; justify-content: space-between; gap: 12px; padding: 6px 10px; background: white; border-radius: 4px; margin-bottom: 4px; font-size: 13px;",
                            span { style: "font-family: monospace; color: #1f2937;", "pub {field.name}" }
                            span { style: "font-family: monospace; color: #6b7280;", "{field.ty}" }
                        }
                    }
                    if !ctx.uses.is_empty() {
                        div { style: "margin-top: 12px; font-size: 12px; color: #6b7280; margin-bottom: 6px;", "From [[context.uses]]" }
                        for u in &ctx.uses {
                            div {
                                style: "display: flex; justify-content: space-between; gap: 12px; padding: 6px 10px; background: #eff6ff; border-radius: 4px; margin-bottom: 4px; font-size: 13px;",
                                span { style: "font-family: monospace; color: #1e40af;", "pub {u.name}" }
                                span { style: "font-family: monospace; color: #6b7280;", "{u.ty}" }
                            }
                        }
                    }
                } else {
                    div { style: "color: #9ca3af; font-size: 13px;", "No context definition (message crate)" }
                }
            }

            // Actions
            div {
                style: "min-width: 380px; background: #f9fafb; border-radius: 8px; padding: 16px;",
                h3 { style: "margin: 0 0 12px 0; color: #1f2937; font-size: 15px;", "Actions" }
                if spec.actions.is_empty() {
                    div { style: "color: #9ca3af; font-size: 13px;", "No action declarations" }
                }
                for action in &spec.actions {
                    div {
                        style: "margin-bottom: 8px; padding: 8px 10px; background: white; border-radius: 4px;",
                        div { style: "font-size: 11px; color: #6b7280;", "{action.crate_name}" }
                        div { style: "font-family: monospace; font-size: 12px; color: #374151; white-space: pre-wrap; word-break: break-all;", "{action.signature}" }
                    }
                }
            }

            // Messages
            div {
                style: "min-width: 280px; background: #f9fafb; border-radius: 8px; padding: 16px;",
                h3 { style: "margin: 0 0 12px 0; color: #1f2937; font-size: 15px;", "Messages" }
                if spec.messages.is_empty() {
                    div { style: "color: #9ca3af; font-size: 13px;", "No message definitions here — see the *-messages spec tabs" }
                }
                for msg in &spec.messages {
                    div {
                        style: "margin-bottom: 10px; padding: 10px; background: white; border-radius: 4px;",
                        div { style: "font-size: 11px; color: #6b7280;", "{msg.crate_name}" }
                        div { style: "font-family: monospace; font-weight: 600; color: #1f2937; margin-bottom: 6px;", "pub enum {msg.enum_name}" }
                        for v in &msg.variants {
                            div {
                                style: "font-family: monospace; font-size: 12px; color: #374151; padding: 2px 0;",
                                "{v.name}"
                                if !v.fields.is_empty() {
                                    span { style: "color: #6b7280;", "({v.fields.join(\", \")})" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Supervision tree view (#123): supervisor hierarchy with policies ───────

#[component]
fn SupervisionTreeView(spec: BloxSpec) -> Element {
    let Some(wiring) = &spec.wiring else {
        return rsx! {
            div {
                style: "padding: 40px; text-align: center; color: #6b7280; font-size: 14px;",
                "This spec has no supervision data. "
                "Select a system spec tab (an app like tokio-pool-demo) to see its supervision tree."
            }
        };
    };

    if wiring.supervisors.is_empty() {
        return rsx! {
            div {
                style: "padding: 40px; text-align: center; color: #6b7280; font-size: 14px;",
                "No [[supervision]] sections in this system."
            }
        };
    }

    rsx! {
        div {
            style: "display: flex; gap: 24px; align-items: flex-start; padding: 12px;",
            for sup in &wiring.supervisors {
                div {
                    style: "background: #f9fafb; border-radius: 8px; padding: 16px; min-width: 280px;",
                    // Supervisor node
                    div {
                        style: "padding: 12px 16px; background: #6366f1; color: white; border-radius: 6px; text-align: center; margin-bottom: 4px;",
                        div { style: "font-weight: 700; font-size: 14px;", "{sup.name}" }
                        div { style: "font-size: 11px; opacity: 0.85;", "strategy: {sup.strategy}" }
                    }
                    // Failure escalation arrow
                    div {
                        style: "text-align: center; color: #9ca3af; font-size: 12px; margin: 4px 0;",
                        "▲ child failure events (Started / Stopped / Done / Failed / Aborted / Killed)"
                    }
                    // Children with policy badges
                    for child in &sup.children {
                        div {
                            style: "display: flex; justify-content: space-between; align-items: center; gap: 10px; padding: 10px 12px; background: white; border-radius: 4px; margin-top: 8px; border: 1px solid #e5e7eb;",
                            span { style: "font-family: monospace; font-size: 13px; color: #1f2937;", "{child.actor}" }
                            span {
                                style: "display: flex; gap: 6px;",
                                if let Some(max) = child.restart_max {
                                    span {
                                        style: "padding: 2px 8px; background: #dcfce7; color: #166534; border-radius: 9999px; font-size: 11px; font-weight: 600;",
                                        "restart: max {max}"
                                    }
                                }
                                if child.stop == Some(true) {
                                    span {
                                        style: "padding: 2px 8px; background: #fee2e2; color: #991b1b; border-radius: 9999px; font-size: 11px; font-weight: 600;",
                                        "stop"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Editor panel (#96): add states / transitions from the UI ───────────────

fn parse_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[component]
fn EditorPanel(spec: BloxSpec, specs: Signal<Vec<BloxSpec>>) -> Element {
    let path = format!("{}/blox.toml", spec.crate_path);
    let path_add_state = path.clone();
    let path_add_transition = path.clone();
    let mut show_state_form = use_signal(|| false);
    let mut show_trans_form = use_signal(|| false);
    let mut edit_status = use_signal(|| None::<String>);

    // State form fields
    let mut state_name = use_signal(String::new);
    let mut state_parent = use_signal(String::new);
    let mut state_composite = use_signal(|| false);
    let mut state_error = use_signal(|| false);

    // Transition form fields
    let mut t_state = use_signal(String::new);
    let mut t_event = use_signal(String::new);
    let mut t_target = use_signal(String::new);
    let mut t_actions = use_signal(String::new);
    let mut t_guards = use_signal(String::new);

    let input_style = "padding: 6px 10px; border: 1px solid #d1d5db; border-radius: 4px; font-size: 13px; font-family: monospace;";
    let btn_style = "padding: 6px 14px; background: #6366f1; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 13px;";

    rsx! {
        div {
            style: "margin-bottom: 12px; display: flex; gap: 8px; align-items: center; flex-wrap: wrap;",
            button {
                style: "{btn_style}",
                onclick: move |_| show_state_form.set(!show_state_form()),
                "+ State"
            }
            button {
                style: "{btn_style}",
                onclick: move |_| show_trans_form.set(!show_trans_form()),
                "+ Transition"
            }
            if let Some(status) = edit_status() {
                span { style: "font-size: 12px; color: #6b7280;", "{status}" }
            }
        }
        if show_state_form() {
            div {
                style: "margin-bottom: 12px; padding: 12px; background: #f9fafb; border-radius: 6px; display: flex; gap: 8px; align-items: center; flex-wrap: wrap;",
                input {
                    style: "{input_style}",
                    placeholder: "StateName",
                    value: "{state_name}",
                    oninput: move |e| state_name.set(e.value()),
                }
                input {
                    style: "{input_style}",
                    placeholder: "parent (optional)",
                    value: "{state_parent}",
                    oninput: move |e| state_parent.set(e.value()),
                }
                label {
                    style: "font-size: 13px; color: #374151;",
                    input {
                        r#type: "checkbox",
                        checked: "{state_composite}",
                        onchange: move |e| state_composite.set(e.checked()),
                    }
                    " composite"
                }
                label {
                    style: "font-size: 13px; color: #374151;",
                    input {
                        r#type: "checkbox",
                        checked: "{state_error}",
                        onchange: move |e| state_error.set(e.checked()),
                    }
                    " error"
                }
                button {
                    style: "{btn_style}",
                    onclick: move |_| {
                        let name = state_name();
                        let parent = {
                            let p = state_parent();
                            if p.is_empty() { None } else { Some(p) }
                        };
                        let (composite, error) = (state_composite(), state_error());
                        let path_add_state = path_add_state.clone();
                        async move {
                            if name.is_empty() {
                                edit_status.set(Some("state name required".to_string()));
                                return;
                            }
                            match viz_add_state(path_add_state, name.clone(), parent, composite, error).await {
                                Ok(new_specs) => {
                                    specs.set(new_specs);
                                    edit_status.set(Some(format!("added state {}", name)));
                                    state_name.set(String::new());
                                    state_parent.set(String::new());
                                }
                                Err(e) => edit_status.set(Some(format!("error: {}", e))),
                            }
                        }
                    },
                    "Add State"
                }
            }
        }
        if show_trans_form() {
            div {
                style: "margin-bottom: 12px; padding: 12px; background: #f9fafb; border-radius: 6px; display: flex; gap: 8px; align-items: center; flex-wrap: wrap;",
                input {
                    style: "{input_style}",
                    placeholder: "from state",
                    value: "{t_state}",
                    oninput: move |e| t_state.set(e.value()),
                }
                input {
                    style: "{input_style} min-width: 280px;",
                    placeholder: "event pattern (e.g. MyMsg::Tick(_))",
                    value: "{t_event}",
                    oninput: move |e| t_event.set(e.value()),
                }
                input {
                    style: "{input_style}",
                    placeholder: "target (state | stay | done | stop | reset | fail)",
                    value: "{t_target}",
                    oninput: move |e| t_target.set(e.value()),
                }
                input {
                    style: "{input_style}",
                    placeholder: "actions (csv, optional)",
                    value: "{t_actions}",
                    oninput: move |e| t_actions.set(e.value()),
                }
                input {
                    style: "{input_style} min-width: 220px;",
                    placeholder: "guards (csv cond:target, optional)",
                    value: "{t_guards}",
                    oninput: move |e| t_guards.set(e.value()),
                }
                button {
                    style: "{btn_style}",
                    onclick: move |_| {
                        let (state, event, target) = (t_state(), t_event(), t_target());
                        let actions = parse_csv(&t_actions());
                        let guards = parse_csv(&t_guards());
                        let path_add_transition = path_add_transition.clone();
                        async move {
                            if state.is_empty() || event.is_empty() || target.is_empty() {
                                edit_status.set(Some("state, event, and target are required".to_string()));
                                return;
                            }
                            match viz_add_transition(path_add_transition, state.clone(), event.clone(), target, actions, guards).await {
                                Ok(new_specs) => {
                                    specs.set(new_specs);
                                    edit_status.set(Some(format!("added transition {} + {}", state, event)));
                                }
                                Err(e) => edit_status.set(Some(format!("error: {}", e))),
                            }
                        }
                    },
                    "Add Transition"
                }
            }
        }
    }
}
