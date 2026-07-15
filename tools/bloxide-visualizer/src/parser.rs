// Copyright 2025 Bloxide, all rights reserved
use crate::model;
use crate::model::*;
use std::collections::HashMap;

pub fn parse_spec(name: &str, markdown: &str) -> BloxSpec {
    let mut states = Vec::new();
    let mut events = Vec::new();
    let mut handlers = Vec::new();
    let mut entry_exit = HashMap::new();
    let mut message_sets = Vec::new();

    // Phase 1: Extract mermaid diagrams and section boundaries
    let mut current_section = String::new();
    let mut in_mermaid = false;
    let mut mermaid_content = String::new();
    let mut section_boundaries: Vec<(String, usize)> = Vec::new(); // (section_name, line_number)

    for (line_num, line) in markdown.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("## ") {
            current_section = trimmed.trim_start_matches("## ").trim().to_string();
            section_boundaries.push((current_section.clone(), line_num));
        }

        if trimmed.starts_with("```mermaid") {
            in_mermaid = true;
            continue;
        }
        if trimmed.starts_with("```") && in_mermaid {
            parse_mermaid(&mermaid_content, &mut states);
            in_mermaid = false;
            mermaid_content.clear();
            continue;
        }
        if in_mermaid {
            mermaid_content.push_str(line);
            mermaid_content.push('\n');
        }
    }

    // Phase 2: Parse tables line-by-line within each section
    for i in 0..section_boundaries.len() {
        let (section_name, start_line) = &section_boundaries[i];
        let end_line = if i + 1 < section_boundaries.len() {
            section_boundaries[i + 1].1
        } else {
            markdown.lines().count()
        };

        let section_lines: Vec<&str> = markdown
            .lines()
            .skip(*start_line)
            .take(end_line - start_line)
            .collect();

        // Find and parse tables in this section
        parse_tables_in_section(
            section_name,
            &section_lines,
            &mut states,
            &mut events,
            &mut handlers,
            &mut entry_exit,
        );
    }

    // Compute state depths and set default parents from mermaid
    compute_hierarchy(&mut states);

    // Fill in inherited handlers for composite state children
    fill_inherited_handlers(&mut handlers, &states);

    // Fill dropped handlers for cells with no explicit or inherited handler
    fill_dropped_handlers(&mut handlers, &states, &events);

    // Build message sets from events
    let mut sets: HashMap<String, Vec<String>> = HashMap::new();
    for event in &events {
        sets.entry(event.message_set.clone())
            .or_default()
            .push(event.variant.clone());
    }
    message_sets = sets
        .into_iter()
        .map(|(name, variants)| MessageSet { name, variants })
        .collect();

    BloxSpec {
        name: name.to_string(),
        states,
        events,
        handlers,
        entry_exit,
        message_sets,
        wiring: None,
    }
}

fn parse_mermaid(content: &str, states: &mut Vec<State>) {
    let mut current_composite: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("stateDiagram") {
            continue;
        }

        if line.starts_with("state ") && line.contains("{") {
            // Composite state: state Operating { ... }
            let name = line
                .trim_start_matches("state ")
                .split('{')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !name.is_empty() && !states.iter().any(|s| s.name == name) {
                states.push(State {
                    name: name.clone(),
                    kind: StateKind::Composite,
                    parent: None,
                    description: String::new(),
                    depth: 0,
                });
                current_composite = Some(name);
            }
        } else if line == "}" {
            current_composite = None;
        } else if line.contains(" --> ") && !line.starts_with("[*]") {
            // State transition: Active --> Paused
            let parts: Vec<&str> = line.split("-->").collect();
            if parts.len() == 2 {
                let source = parts[0].trim().to_string();
                let target_part = parts[1].trim();
                let target = target_part
                    .split(':')
                    .next()
                    .unwrap_or(target_part)
                    .trim()
                    .to_string();

                for state_name in [source, target] {
                    let clean_name = state_name.trim_start_matches("[*] --> ").trim().to_string();
                    if !clean_name.is_empty()
                        && !clean_name.starts_with("[*]")
                        && !states.iter().any(|s| s.name == clean_name)
                    {
                        states.push(State {
                            name: clean_name,
                            kind: StateKind::Leaf,
                            parent: current_composite.clone(),
                            description: String::new(),
                            depth: 0,
                        });
                    }
                }
            }
        } else if !line.contains("-->")
            && !line.contains("{")
            && !line.starts_with("}")
            && !line.is_empty()
            && !line.starts_with("note")
        {
            // Maybe a standalone state name (child of composite)
            let name = line.trim().to_string();
            if !name.is_empty() && !states.iter().any(|s| s.name == name) {
                states.push(State {
                    name: name.clone(),
                    kind: StateKind::Leaf,
                    parent: current_composite.clone(),
                    description: String::new(),
                    depth: 0,
                });
            }
        }
    }
}

fn parse_tables_in_section(
    section: &str,
    lines: &[&str],
    states: &mut Vec<State>,
    events: &mut Vec<model::Event>,
    handlers: &mut Vec<Handler>,
    entry_exit: &mut HashMap<String, EntryExit>,
) {
    // Find table blocks (lines starting with |)
    let mut current_table: Vec<Vec<String>> = Vec::new();
    let mut in_table = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with('|') && !trimmed.starts_with("|>") {
            // This is a table row
            let cells: Vec<String> = trimmed
                .trim_start_matches('|')
                .trim_end_matches('|')
                .split('|')
                .map(|s| s.trim().to_string())
                .collect();

            // Skip separator rows (all dashes)
            if cells.iter().all(|c| {
                c.chars()
                    .all(|ch| ch == '-' || ch == ' ' || ch == ':' || ch == '|')
            }) {
                continue;
            }

            current_table.push(cells);
            in_table = true;
        } else if in_table && !trimmed.starts_with('|') {
            // Table ended
            if !current_table.is_empty() {
                parse_table(
                    section,
                    &current_table,
                    states,
                    events,
                    handlers,
                    entry_exit,
                );
            }
            current_table.clear();
            in_table = false;
        }
    }

    // Handle table at end of section
    if !current_table.is_empty() {
        parse_table(
            section,
            &current_table,
            states,
            events,
            handlers,
            entry_exit,
        );
    }
}

fn parse_table(
    section: &str,
    rows: &[Vec<String>],
    states: &mut Vec<State>,
    events: &mut Vec<model::Event>,
    handlers: &mut Vec<Handler>,
    entry_exit: &mut HashMap<String, EntryExit>,
) {
    if rows.len() < 2 {
        return;
    }

    match section.trim() {
        "States" => {
            for row in &rows[1..] {
                if row.len() < 3 {
                    continue;
                }
                let name = strip_backticks(&row[0]);
                let kind_str = row.get(1).map(|s| s.trim().to_string()).unwrap_or_default();
                let description = row.get(2).map(|s| s.trim().to_string()).unwrap_or_default();

                if name.is_empty() || name == "State" || name.starts_with('[') {
                    continue;
                }

                let kind = if kind_str.contains("composite") {
                    StateKind::Composite
                } else if kind_str.contains("terminal") {
                    StateKind::Terminal
                } else if kind_str.contains("error") {
                    StateKind::Error
                } else {
                    StateKind::Leaf
                };

                // Update or insert
                if let Some(existing) = states.iter_mut().find(|s| s.name == name) {
                    existing.kind = kind;
                    existing.description = description;
                } else {
                    states.push(State {
                        name,
                        kind,
                        parent: None,
                        description,
                        depth: 0,
                    });
                }
            }
        }
        "Events" => {
            // Determine column layout from header
            let headers: Vec<String> = rows[0].iter().map(|s| s.trim().to_lowercase()).collect();
            let event_col = headers.iter().position(|h| h.contains("event"));
            let handled_by_col = headers
                .iter()
                .position(|h| h.contains("handled") || h.contains("by"));
            let guard_col = headers.iter().position(|h| {
                h.contains("guard") || h.contains("outcome") || h.contains("reaction")
            });
            let side_effects_col = headers
                .iter()
                .position(|h| h.contains("side") || h.contains("effects"));

            for row in &rows[1..] {
                if row.len() < 2 {
                    continue;
                }

                let event_str = event_col
                    .map(|i| strip_backticks(&row[i]))
                    .unwrap_or_default();
                let handled_by = handled_by_col
                    .map(|i| strip_backticks(&row[i]))
                    .unwrap_or_default();
                let guard_outcome = guard_col
                    .map(|i| row[i].trim().to_string())
                    .unwrap_or_default();
                let side_effects = side_effects_col
                    .map(|i| row[i].trim().to_string())
                    .unwrap_or_default();

                if event_str == "Event" || event_str.is_empty() || event_str == "any unhandled" {
                    continue;
                }

                // Parse event: PingPongMsg::Pong(_) or CounterMsg::Tick
                let (message_set, variant) = parse_event_str(&event_str);
                let full_name = format!("{}::{}", message_set, variant);

                if !events.iter().any(|e| e.full_name == full_name) {
                    events.push(model::Event {
                        message_set: message_set.clone(),
                        variant: variant.clone(),
                        full_name: full_name.clone(),
                    });
                }

                // Create handler if handled by a specific state
                if !handled_by.is_empty() && handled_by != "Handled by" {
                    let state_name = if handled_by.contains("→") {
                        // e.g., "Paused → bubbles → Operating"
                        let first = handled_by.split("→").next().unwrap_or(&handled_by).trim();
                        strip_backticks(first)
                    } else {
                        strip_backticks(&handled_by)
                    };

                    let (actions, guard, target, label) = parse_rule(&guard_outcome, &side_effects);

                    if !handlers
                        .iter()
                        .any(|h| h.state == state_name && h.event == full_name)
                    {
                        handlers.push(Handler {
                            state: state_name,
                            event: full_name,
                            label,
                            actions,
                            guard,
                            target,
                            source: HandlerSource::Explicit,
                            on_entry: Vec::new(),
                            on_exit: Vec::new(),
                        });
                    }
                }
            }
        }
        "Entry / Exit Actions" | "Entry/Exit Actions" | "Entry / Exit" => {
            for row in &rows[1..] {
                if row.len() < 3 {
                    continue;
                }
                let state_name = strip_backticks(&row[0]);
                let on_entry_str = row.get(1).map(|s| s.trim().to_string()).unwrap_or_default();
                let on_exit_str = row.get(2).map(|s| s.trim().to_string()).unwrap_or_default();

                if state_name == "State" || state_name.is_empty() || state_name.starts_with('[') {
                    continue;
                }

                let on_entry = if on_entry_str.is_empty()
                    || on_entry_str == "—"
                    || on_entry_str == "_"
                    || on_entry_str == "(none)"
                {
                    Vec::new()
                } else {
                    on_entry_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect()
                };

                let on_exit = if on_exit_str.is_empty()
                    || on_exit_str == "—"
                    || on_exit_str == "_"
                    || on_exit_str == "(none)"
                {
                    Vec::new()
                } else {
                    on_exit_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect()
                };

                entry_exit.insert(state_name, EntryExit { on_entry, on_exit });
            }
        }
        _ => {}
    }
}

fn strip_backticks(s: &str) -> String {
    s.replace('`', "").trim().to_string()
}

fn parse_event_str(event_str: &str) -> (String, String) {
    let cleaned = event_str.trim().trim_end_matches("(_)").trim().to_string();

    if let Some(pos) = cleaned.find("::") {
        let message_set = cleaned[..pos].trim().to_string();
        let variant = cleaned[pos + 2..].trim().to_string();
        // Remove any remaining parenthetical content
        let variant = variant
            .split('(')
            .next()
            .unwrap_or(&variant)
            .trim()
            .to_string();
        (message_set, variant)
    } else {
        ("Unknown".to_string(), cleaned)
    }
}

fn parse_rule(guard_outcome: &str, side_effects: &str) -> (Vec<String>, Guard, Target, String) {
    let actions: Vec<String> = if side_effects.is_empty()
        || side_effects == "none"
        || side_effects == "none "
        || side_effects == "—"
    {
        Vec::new()
    } else {
        side_effects
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    let guard = Guard {
        description: guard_outcome.to_string(),
        raw: guard_outcome.to_string(),
        branches: Vec::new(),
    };

    let clean_guard = strip_backticks(guard_outcome);
    let target = if clean_guard.contains("Stay") {
        Target::Stay
    } else if clean_guard.contains("Transition") {
        // Extract state name from "Transition(StateName)"
        if let Some(start) = clean_guard.find("Transition(") {
            let after = &clean_guard[start + "Transition(".len()..];
            let end = after.find(')').unwrap_or(after.len());
            let state_name = after[..end].trim().to_string();
            Target::Transition(state_name)
        } else {
            Target::Stay
        }
    } else if clean_guard.contains("reset") || clean_guard.contains("Reset") {
        Target::Reset
    } else {
        Target::Stay
    };

    let label = if actions.is_empty() {
        target.display()
    } else {
        // Use a short label: first action name or count
        let action_label = if actions.len() == 1 {
            actions[0].clone()
        } else {
            format!("{} actions", actions.len())
        };
        format!("{} → {}", action_label, target.display())
    };

    (actions, guard, target, label)
}

fn compute_hierarchy(states: &mut Vec<State>) {
    // Set depth based on parent relationships
    // Composite states are depth 0, their children are depth 1
    for state in states.iter_mut() {
        if state.parent.is_some() {
            state.depth = 1;
        }
    }
}

fn fill_inherited_handlers(handlers: &mut Vec<Handler>, states: &[State]) {
    // For each composite state, copy its handlers to children
    let composites: Vec<&State> = states
        .iter()
        .filter(|s| matches!(s.kind, StateKind::Composite))
        .collect();

    for composite in &composites {
        let composite_handlers: Vec<Handler> = handlers
            .iter()
            .filter(|h| h.state == composite.name)
            .cloned()
            .collect();

        let children: Vec<&State> = states
            .iter()
            .filter(|s| s.parent.as_ref() == Some(&composite.name))
            .collect();

        for child in children {
            for ch in &composite_handlers {
                if !handlers
                    .iter()
                    .any(|h| h.state == child.name && h.event == ch.event)
                {
                    handlers.push(Handler {
                        state: child.name.clone(),
                        event: ch.event.clone(),
                        label: format!("⬇️ {} ({})", ch.label, composite.name),
                        actions: ch.actions.clone(),
                        guard: ch.guard.clone(),
                        target: ch.target.clone(),
                        source: HandlerSource::Inherited(composite.name.clone()),
                        on_entry: Vec::new(),
                        on_exit: Vec::new(),
                    });
                }
            }
        }
    }
}

fn fill_dropped_handlers(handlers: &mut Vec<Handler>, states: &[State], events: &[model::Event]) {
    // For leaf states that don't have a handler for an event, add a Dropped handler
    let leaf_states: Vec<&State> = states.iter().filter(|s| s.kind.is_leaf()).collect();

    for state in leaf_states {
        for event in events {
            if !handlers
                .iter()
                .any(|h| h.state == state.name && h.event == event.full_name)
            {
                handlers.push(Handler {
                    state: state.name.clone(),
                    event: event.full_name.clone(),
                    label: "∅".to_string(),
                    actions: Vec::new(),
                    guard: Guard {
                        description: "No handler — dropped".to_string(),
                        raw: String::new(),
                        branches: Vec::new(),
                    },
                    target: Target::Stay,
                    source: HandlerSource::Dropped,
                    on_entry: Vec::new(),
                    on_exit: Vec::new(),
                });
            }
        }
    }
}
