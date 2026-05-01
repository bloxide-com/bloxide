// Copyright 2025 Bloxide, all rights reserved
//! Shared utilities for cargo-blox subcommands.

use std::fs;
use std::path::Path;
use anyhow::Result;

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

pub fn update_workspace_cargo_toml(additions: &[WorkspaceAddition]) -> Result<()> {
    let root_cargo = Path::new("Cargo.toml");
    let content = fs::read_to_string(root_cargo)?;
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
    fs::write(root_cargo, new_content)?;
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

pub fn generate_spec_md(name_snake: &str, name_camel: &str) -> String {
    let template_path = Path::new("spec/templates/blox-spec.md");
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

- Blox crate: `crates/bloxes/<blox-name>/`
- Messages crate: `crates/messages/<blox-name>-messages/`
- Actions crate: `crates/actions/<blox-name>-actions/`

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

| Event | Handled by | Rule pattern | Guard outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| any unhandled | root | — | dropped | none |

## Context

```rust
#[derive(BloxCtx)]
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

- [ ] `machine.start()` enters `Ready`
- [ ] `is_terminal(&State::Done)` returns `true`
"#
}
