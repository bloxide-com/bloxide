// Copyright 2025 Bloxide, all rights reserved
pub mod model;
pub mod parser;

use quote::ToTokens;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub use model::BloxSpec;

/// Scan a workspace for blox crates and export them as `BloxSpec` structs.
///
/// Returns one `BloxSpec` per discovered blox crate.
pub fn export_workspace(workspace_path: &Path) -> Result<Vec<BloxSpec>, String> {
    let blox_crates = find_blox_crates(workspace_path);

    if blox_crates.is_empty() {
        return Err("No blox crates found.".to_string());
    }

    let mut specs = Vec::new();

    for (name, crate_path) in &blox_crates {
        let spec_path = crate_path.join("src/spec.rs");
        let events_path = crate_path.join("src/events.rs");
        let ctx_path = crate_path.join("src/ctx.rs");

        let source = fs::read_to_string(&spec_path)
            .map_err(|e| format!("could not read {}: {}", spec_path.display(), e))?;

        if source.is_empty() {
            continue;
        }

        let mut spec = parser::parse_blox_file(name, &crate_path.to_string_lossy(), &source)?;

        // Try to parse events.rs for additional event/message info
        if events_path.exists() {
            if let Ok(events_source) = fs::read_to_string(&events_path) {
                extract_messages_from_events(&mut spec, &events_source);
            }
        }

        // Try to parse ctx.rs for context fields
        if ctx_path.exists() {
            if let Ok(ctx_source) = fs::read_to_string(&ctx_path) {
                extract_context_fields(&mut spec, &ctx_source);
            }
        }

        specs.push(spec);
    }

    Ok(specs)
}

/// Write exported specs as JSON files to the given output directory.
pub fn write_specs_to_json(specs: &[BloxSpec], output_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    for spec in specs {
        let output_path = output_dir.join(format!("{}.json", spec.name.to_lowercase()));
        let json = serde_json::to_string_pretty(spec)
            .map_err(|e| format!("Failed to serialize JSON: {}", e))?;
        fs::write(&output_path, json)
            .map_err(|e| format!("Failed to write {}: {}", output_path.display(), e))?;
    }

    Ok(())
}

fn find_blox_crates(workspace_path: &Path) -> Vec<(String, PathBuf)> {
    let mut crates = Vec::new();

    for entry in WalkDir::new(workspace_path)
        .max_depth(6)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.file_name() == Some(std::ffi::OsStr::new("spec.rs")) {
            let crate_path = path.parent().and_then(|p| p.parent());
            if let Some(crate_path) = crate_path {
                if let Ok(content) = fs::read_to_string(path) {
                    if content.contains("MachineSpec for") && content.contains("StateTopology") {
                        let blox_name = crate_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| {
                                let mut chars = n.chars();
                                match chars.next() {
                                    None => String::new(),
                                    Some(first) => {
                                        first.to_uppercase().collect::<String>() + chars.as_str()
                                    }
                                }
                            })
                            .unwrap_or_else(|| "Unknown".to_string());

                        crates.push((blox_name, crate_path.to_path_buf()));
                    }
                }
            }
        }
    }

    crates.sort_by(|a, b| a.0.cmp(&b.0));
    crates.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    crates
}

fn extract_messages_from_events(spec: &mut BloxSpec, source: &str) {
    if let Ok(file) = syn::parse_file(source) {
        for item in &file.items {
            if let syn::Item::Enum(item_enum) = item {
                let has_blox_event = item_enum.attrs.iter().any(|attr| {
                    attr.path()
                        .get_ident()
                        .map(|i| i == "blox_event")
                        .unwrap_or(false)
                });

                if has_blox_event {
                    for variant in &item_enum.variants {
                        let _variant_name = variant.ident.to_string();
                        if let syn::Fields::Unnamed(fields) = &variant.fields {
                            if let Some(field) = fields.unnamed.first() {
                                let ty_str = format_type(&field.ty);
                                if let Some(start) = ty_str.find('<') {
                                    if let Some(end) = ty_str.rfind('>') {
                                        let msg_type = &ty_str[start + 1..end];
                                        if !spec.messages.iter().any(|m| m.enum_name == msg_type) {
                                            spec.messages.push(model::MessageDef {
                                                crate_name: "unknown".to_string(),
                                                enum_name: msg_type.to_string(),
                                                variants: Vec::new(),
                                            });
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
}

fn extract_context_fields(spec: &mut BloxSpec, source: &str) {
    if let Ok(file) = syn::parse_file(source) {
        for item in &file.items {
            if let syn::Item::Struct(item_struct) = item {
                let has_blox_ctx = item_struct.attrs.iter().any(|attr| {
                    attr.path()
                        .get_ident()
                        .map(|i| i == "derive")
                        .unwrap_or(false)
                        && attr.parse_args::<syn::Expr>().ok().map_or(false, |e| {
                            e.to_token_stream().to_string().contains("BloxCtx")
                        })
                });

                if has_blox_ctx {
                    let mut fields = Vec::new();
                    for field in &item_struct.fields {
                        let name = field
                            .ident
                            .as_ref()
                            .map(|i| i.to_string())
                            .unwrap_or_default();
                        let ty = format_type(&field.ty);
                        let annotations: Vec<String> = field
                            .attrs
                            .iter()
                            .map(|attr| attr.to_token_stream().to_string())
                            .collect();

                        fields.push(model::ContextField {
                            name,
                            ty,
                            annotations,
                        });
                    }

                    spec.context = Some(model::ContextDef {
                        struct_name: item_struct.ident.to_string(),
                        fields,
                    });
                }
            }
        }
    }
}

fn format_type(ty: &syn::Type) -> String {
    ty.to_token_stream().to_string()
}
