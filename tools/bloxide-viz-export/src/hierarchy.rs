// Copyright 2025 Bloxide, all rights reserved

//! Hierarchy and handler post-processing.

use crate::model;
use std::collections::HashMap;

pub(crate) fn compute_hierarchy(states: &mut [model::State]) {
    // Two-pass: build a parent lookup, then walk each state's ancestor
    // chain so that depth reflects true nesting (S211 → S21 → S2 → S is
    // depth 3, not 1). The walk is bounded by the state count so a
    // malformed parent cycle cannot loop forever.
    let parent_of: HashMap<&str, Option<&str>> = states
        .iter()
        .map(|s| (s.name.as_str(), s.parent.as_deref()))
        .collect();

    let depths: Vec<usize> = states
        .iter()
        .map(|s| {
            let mut depth = 0;
            let mut ancestor = s.parent.as_deref();
            while let Some(name) = ancestor {
                depth += 1;
                if depth > parent_of.len() {
                    break;
                }
                ancestor = parent_of.get(name).copied().flatten();
            }
            depth
        })
        .collect();

    for (state, depth) in states.iter_mut().zip(depths) {
        state.depth = depth;
    }
}

pub(crate) fn fill_inherited_handlers(handlers: &mut Vec<model::Handler>, states: &[model::State]) {
    let composites: Vec<&model::State> = states
        .iter()
        .filter(|s| matches!(s.kind, model::StateKind::Composite))
        .collect();

    for composite in &composites {
        let composite_handlers: Vec<model::Handler> = handlers
            .iter()
            .filter(|h| h.state == composite.name)
            .cloned()
            .collect();

        let children: Vec<&model::State> = states
            .iter()
            .filter(|s| s.parent.as_ref() == Some(&composite.name))
            .collect();

        for child in children {
            for ch in &composite_handlers {
                if !handlers
                    .iter()
                    .any(|h| h.state == child.name && h.event == ch.event)
                {
                    handlers.push(model::Handler {
                        state: child.name.clone(),
                        event: ch.event.clone(),
                        label: format!("⬇️ {} ({})", ch.label, composite.name),
                        pattern: ch.pattern.clone(),
                        feature: ch.feature.clone(),
                        actions: ch.actions.clone(),
                        guard: ch.guard.clone(),
                        target: ch.target.clone(),
                        source: model::HandlerSource::Inherited(composite.name.clone()),
                        on_entry: Vec::new(),
                        on_exit: Vec::new(),
                    });
                }
            }
        }
    }
}

pub(crate) fn fill_dropped_handlers(
    handlers: &mut Vec<model::Handler>,
    states: &[model::State],
    events: &[model::Event],
) {
    let leaf_states: Vec<&model::State> = states.iter().filter(|s| s.kind.is_leaf()).collect();

    for state in leaf_states {
        for event in events {
            if !handlers
                .iter()
                .any(|h| h.state == state.name && h.event == event.full_name)
            {
                handlers.push(model::Handler {
                    state: state.name.clone(),
                    event: event.full_name.clone(),
                    label: "∅".to_string(),
                    pattern: String::new(),
                    feature: None,
                    actions: Vec::new(),
                    guard: model::Guard {
                        description: "No handler — dropped".to_string(),
                        raw: String::new(),
                        branches: Vec::new(),
                    },
                    target: model::Target::Stay,
                    source: model::HandlerSource::Dropped,
                    on_entry: Vec::new(),
                    on_exit: Vec::new(),
                });
            }
        }
    }
}
