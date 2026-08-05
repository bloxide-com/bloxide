// Copyright 2025 Bloxide, all rights reserved

//! File discovery.

use bloxide_codegen::schema::BloxConfig;
use std::fs;
use std::path::{Path, PathBuf};

pub fn find_blox_tomls(workspace_path: &Path) -> Vec<(String, PathBuf)> {
    let mut crates = Vec::new();

    for entry in walkdir_tomls(workspace_path) {
        let path = entry;
        if path.file_name() != Some(std::ffi::OsStr::new("blox.toml")) {
            continue;
        }

        let crate_path = path.parent();

        // Skip if this is inside a target/ build directory
        if path
            .components()
            .any(|c| c.as_os_str() == std::ffi::OsStr::new("target"))
        {
            continue;
        }

        if let Some(crate_path) = crate_path {
            if let Ok(content) = fs::read_to_string(&path) {
                // Actor/topology crates and message-definition crates both
                // produce specs (#124 — message definitions are renderable).
                if content.contains("[actor]")
                    || content.contains("[topology]")
                    || content.contains("[[messages]]")
                {
                    let dir_name = crate_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown");

                    // Try to get the actor name from the TOML; fall back to
                    // Pascal-casing the directory name
                    let name = extract_actor_name(&content).unwrap_or_else(|| {
                        let mut chars = dir_name.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(first) => {
                                first.to_uppercase().collect::<String>() + chars.as_str()
                            }
                        }
                    });

                    crates.push((name, path.clone()));
                }
            }
        }
    }

    crates.sort_by(|a, b| a.0.cmp(&b.0));
    crates.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    crates
}

/// Extract the `[actor] name = "..."` value from a blox.toml string without
/// full deserialization (used only for sorting/naming in discovery).
fn extract_actor_name(content: &str) -> Option<String> {
    let config: BloxConfig = toml::from_str(content).ok()?;
    config.actor.map(|a| a.name)
}

/// Walk a directory tree for `blox.toml` files, up to max_depth 6.
pub(crate) fn walkdir_tomls(workspace_path: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    walk_dir_recursive(workspace_path, 0, 6, &mut results);
    results
}

fn walk_dir_recursive(path: &Path, depth: usize, max_depth: usize, results: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();

        // Skip hidden directories and target/
        if let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "target" {
                continue;
            }
        }

        if entry_path.is_dir() {
            walk_dir_recursive(&entry_path, depth + 1, max_depth, results);
        } else if matches!(
            entry_path.file_name().and_then(|n| n.to_str()),
            Some("blox.toml") | Some("system.toml")
        ) {
            results.push(entry_path);
        }
    }
}
