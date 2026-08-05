// Copyright 2025 Bloxide, all rights reserved
//! Dioxus server functions: workspace scanning and blox.toml write-back edits.
use crate::model::*;
use dioxus::prelude::*;
use dioxus_fullstack::server;
use dioxus_fullstack::ServerFnError;

/// Server function: scan a workspace path and return all discovered blox specs.
#[server(endpoint = "api/scan")]
pub(crate) async fn scan_workspace(path: String) -> Result<Vec<BloxSpec>, ServerFnError> {
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

    // Regenerate code from the edited manifest. A codegen failure is fatal:
    // re-exporting anyway would show a spec whose generated code does not
    // compile.
    let output = std::process::Command::new("cargo")
        .args(["blox", "generate"])
        .current_dir(path.parent().unwrap_or(std::path::Path::new(".")))
        .output()
        .map_err(|e| server_err(format!("failed to run `cargo blox generate`: {}", e)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(server_err(format!(
            "`cargo blox generate` failed ({}): {}",
            output.status,
            stderr.trim()
        )));
    }

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
pub(crate) async fn viz_add_state(
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
pub(crate) async fn viz_remove_state(
    path: String,
    name: String,
) -> Result<Vec<BloxSpec>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        apply_edit_and_reexport(&path, |doc| bloxide_codegen::edit::remove_state(doc, &name))
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
pub(crate) async fn viz_add_transition(
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
pub(crate) async fn viz_remove_transition(
    path: String,
    state: String,
    event: String,
) -> Result<Vec<BloxSpec>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        apply_edit_and_reexport(&path, |doc| {
            bloxide_codegen::edit::remove_transition(doc, &state, &event, None)
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
pub(crate) async fn default_specs() -> Result<Vec<BloxSpec>, ServerFnError> {
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
            if !bloxide_viz_export::find_blox_tomls(d).is_empty() {
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
