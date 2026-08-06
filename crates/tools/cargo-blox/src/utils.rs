// Copyright 2025 Bloxide, all rights reserved
//! Shared utilities for cargo-blox subcommands.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Walk up from `start` to find the workspace root (directory containing
/// `Cargo.toml` with a `[workspace]` section).
pub fn find_workspace_root_from(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return Some(current);
                }
            }
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Find the workspace root from the current directory.
pub fn find_workspace_root() -> Result<PathBuf> {
    find_workspace_root_from(&std::env::current_dir()?)
        .ok_or_else(|| anyhow::anyhow!("not inside a Cargo workspace"))
}

/// Resolve the workspace root from the current directory, falling back to
/// the current directory outside a workspace (the watch/generate
/// convention). The `new-*` scaffolding commands anchor all their output
/// paths on this root so they work from any subdirectory.
pub fn workspace_root_or_cwd() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    Ok(find_workspace_root_from(&cwd).unwrap_or(cwd))
}

/// Validate a `--runtime` value for the scaffolding commands (`init`,
/// `new-binary`, `new-all`): only the runtimes the codegen can wire a
/// system.toml for are accepted. Call before writing anything so a bad
/// value fails fast instead of leaving an unwirable system.toml behind.
pub fn validate_runtime(runtime: &str) -> Result<()> {
    if runtime != "tokio" && runtime != "embassy" {
        anyhow::bail!("unknown runtime '{}' — expected tokio or embassy", runtime);
    }
    Ok(())
}

/// Depth limit for recursive workspace discovery of `blox.toml` /
/// `system.toml` manifests. walkdir counts the root as depth 0, so the
/// deepest standard-layout manifest — `crates/<group>/<name>/blox.toml` —
/// sits at depth 4; one extra level (5) covers nested crate layouts. Every
/// discovery walk (generate, lint, verify) MUST share this limit: when the
/// depths drift, a deeply nested blox.toml is linted but never generated.
pub(crate) const DISCOVERY_MAX_DEPTH: usize = 5;

/// Extract the `[package] name = "..."` value from a Cargo.toml file.
#[allow(dead_code)]
pub fn parse_package_name(cargo_toml_path: &Path) -> Option<String> {
    let content = fs::read_to_string(cargo_toml_path).ok()?;
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("name = ") {
            let name = trimmed
                .strip_prefix("name = ")
                .unwrap()
                .trim()
                .trim_matches('"');
            Some(name.to_string())
        } else {
            None
        }
    })
}

/// Discover all `blox.toml` files under `workspace_root` (excluding `target/`),
/// parse each, and return a map keyed by the crate name from the sibling
/// `Cargo.toml`.
#[allow(dead_code)]
pub fn discover_blox_configs(
    workspace_root: &Path,
) -> Result<std::collections::BTreeMap<String, bloxide_codegen::schema::BloxConfig>> {
    use bloxide_codegen::schema::BloxConfig;
    let mut blox_configs = std::collections::BTreeMap::new();
    for entry in walkdir::WalkDir::new(workspace_root)
        .into_iter()
        .filter_entry(|e| e.file_name() != "target")
        .filter_map(|e| e.ok())
    {
        if entry.file_name() == "blox.toml" {
            let blox_content = fs::read_to_string(entry.path())?;
            let blox_config: BloxConfig = toml::from_str(&blox_content)?;
            let dir = entry.path().parent().unwrap();
            let cargo_toml_path = dir.join("Cargo.toml");
            let key = if cargo_toml_path.exists() {
                parse_package_name(&cargo_toml_path)
                    .unwrap_or_else(|| dir.file_name().unwrap().to_string_lossy().to_string())
            } else {
                dir.file_name().unwrap().to_string_lossy().to_string()
            };
            blox_configs.insert(key, blox_config);
        }
    }
    Ok(blox_configs)
}

/// List the available examples: `<root>/examples/<name>/system.toml`
/// sources, by crate name (`[system] name`, falling back to the directory
/// name). Used for `--example` error messages.
pub fn available_examples(root: &Path) -> Vec<String> {
    let examples_dir = root.join("examples");
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(&examples_dir) {
        for entry in entries.flatten() {
            let system_toml = entry.path().join("system.toml");
            if !system_toml.exists() {
                continue;
            }
            let name = fs::read_to_string(&system_toml)
                .ok()
                .and_then(|c| toml::from_str::<bloxide_codegen::schema::SystemConfig>(&c).ok())
                .and_then(|config| config.system.name)
                .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
            names.push(name);
        }
    }
    names.sort();
    names
}

pub fn to_camel_case(name: &str) -> String {
    name.split(['-', '_'])
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

pub enum WorkspaceAddition {
    Member(String),
    Dependency { name: String, toml_line: String },
}

pub fn update_workspace_cargo_toml(root: &Path, additions: &[WorkspaceAddition]) -> Result<()> {
    let root_cargo = root.join("Cargo.toml");
    let content = fs::read_to_string(&root_cargo)?;
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    for addition in additions {
        match addition {
            WorkspaceAddition::Member(path) => {
                if lines.iter().any(|l| l.contains(path.as_str())) {
                    continue;
                }
                let entry = format!("    \"{}\",", path);
                let category_prefix = path
                    .rsplit_once('/')
                    .map(|(p, _)| format!("{}/", p))
                    .unwrap_or_default();
                let insert_idx = find_member_insert_point(&lines, &category_prefix);
                lines.insert(insert_idx, entry);
            }
            WorkspaceAddition::Dependency { name, toml_line } => {
                if lines.iter().any(|l| {
                    let trimmed = l.trim();
                    trimmed.starts_with(name.as_str()) && trimmed.contains('=')
                }) {
                    continue;
                }
                let insert_idx = find_dep_insert_point(&lines, name);
                lines.insert(insert_idx, toml_line.clone());
            }
        }
    }

    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    fs::write(&root_cargo, new_content)?;
    println!("Updated: {}", root_cargo.display());
    Ok(())
}

fn find_member_insert_point(lines: &[String], category_prefix: &str) -> usize {
    let mut members_end = None;
    let mut in_members = false;
    for (i, line) in lines.iter().enumerate() {
        if line.contains("members = [") {
            in_members = true;
            continue;
        }
        if in_members && line.trim() == "]" {
            members_end = Some(i);
            break;
        }
    }
    let members_end = members_end.unwrap_or(lines.len());

    if !category_prefix.is_empty() {
        let mut last_match = None;
        for (i, line) in lines.iter().enumerate().take(members_end) {
            let trimmed = line.trim();
            if trimmed.starts_with('"') && trimmed.contains(category_prefix) {
                last_match = Some(i + 1);
            }
        }
        if let Some(idx) = last_match {
            return idx;
        }
    }

    members_end
}

fn find_dep_insert_point(lines: &[String], dep_name: &str) -> usize {
    let mut dep_section_start = None;
    let mut next_section = lines.len();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "[workspace.dependencies]" {
            dep_section_start = Some(i + 1);
        } else if dep_section_start.is_some()
            && line.trim().starts_with('[')
            && !line.trim().starts_with("[workspace")
        {
            next_section = i;
            break;
        }
    }
    let dep_start = dep_section_start.unwrap_or(lines.len());
    let dep_end = next_section;

    let category_suffix = if dep_name.ends_with("-messages") {
        "-messages"
    } else if dep_name.ends_with("-actions") {
        "-actions"
    } else if dep_name.ends_with("-blox") {
        "-blox"
    } else if dep_name.ends_with("-impl") {
        "-impl"
    } else {
        ""
    };

    if !category_suffix.is_empty() {
        let mut last_match = None;
        for (i, line) in lines.iter().enumerate().take(dep_end).skip(dep_start) {
            let trimmed = line.trim();
            if trimmed.contains(category_suffix) && trimmed.contains('=') {
                last_match = Some(i + 1);
            }
        }
        if let Some(idx) = last_match {
            return idx;
        }
    }

    dep_end
}

pub fn generate_spec_md(root: &Path, name_snake: &str, name_camel: &str) -> String {
    let template_path = root.join("spec/templates/blox-spec.md");
    let template = if template_path.exists() {
        fs::read_to_string(template_path).unwrap_or_else(|_| default_spec_template().into())
    } else {
        default_spec_template().into()
    };

    template
        .replace("<BloxName>", name_camel)
        .replace("<blox-name>", name_snake)
}

pub fn default_spec_template() -> &'static str {
    r#"# Blox Spec: `<BloxName>`

## Purpose

One paragraph. What does this actor do?

## Crate Location

- Blox crate: `bloxes/<blox-name>/`
- Messages crate: `crates/messages/<blox-name>-messages/`
- Context crate: `crates/context/blox-ctx-<name>/` _(action functions; no concrete types)_

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Ready
    Ready --> Done
```

## States

| State | Kind | Description |
|-------|------|-------------|
| `Ready` | leaf | Initial operational state |
| `Done`  | leaf | Terminal state |

## Events

| Event | Handled by | Rule pattern | Decision outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| any unhandled | root | — | dropped | none |

## Context

```rust
pub struct <BloxName>Ctx<R: BloxRuntime> {
    pub self_id: ActorId,
}
```

## Message Contracts

### Receives (`<BloxName>Msg`)

| Variant | Payload | Sent by |
|---------|---------|---------|

### Sends

| Target | Message | When |
|--------|---------|------|

## Acceptance Criteria

- [ ] `LifecycleCommand::Start` enters `Ready` (via dispatch)
- [ ] Actor self-suspends via `Decision::Stop` (goes to Init, reports `Stopped`)
"#
}
