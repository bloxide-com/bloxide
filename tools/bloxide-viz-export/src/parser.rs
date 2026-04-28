// Copyright 2025 Bloxide, all rights reserved
use proc_macro2::{Delimiter, Ident, TokenStream, TokenTree};
use std::collections::HashMap;
use syn::visit::Visit;
use syn::{Expr, ExprArray, ItemConst, ItemEnum, Variant};

use crate::model::*;

pub struct BloxExtractor {
    pub spec: BloxSpec,
    pub state_enum_name: Option<String>,
    pub current_state_fns: Option<String>,
}

impl BloxExtractor {
    pub fn new(name: &str, crate_path: &str) -> Self {
        Self {
            spec: BloxSpec {
                name: name.to_string(),
                crate_path: crate_path.to_string(),
                states: Vec::new(),
                events: Vec::new(),
                handlers: Vec::new(),
                entry_exit: HashMap::new(),
                message_sets: Vec::new(),
                messages: Vec::new(),
                actions: Vec::new(),
                context: None,
            },
            state_enum_name: None,
            current_state_fns: None,
        }
    }
}

impl<'ast> Visit<'ast> for BloxExtractor {
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let has_state_topology = node.attrs.iter().any(|attr| {
            attr.path().is_ident("derive")
                && attr
                    .parse_args::<TokenStream>()
                    .ok()
                    .map_or(false, |ts| ts.to_string().contains("StateTopology"))
        });

        if has_state_topology {
            self.state_enum_name = Some(node.ident.to_string());
            for variant in &node.variants {
                self.extract_state(variant);
            }
        }

        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        // Look for StateFns constants like ACTIVE_FNS, PAUSED_FNS, etc.
        let name = node.ident.to_string();
        if name.ends_with("_FNS") {
            let state_name = name.trim_end_matches("_FNS");
            self.current_state_fns = Some(state_name.to_string());
            self.extract_state_fns(&node.expr);
            self.current_state_fns = None;
        }

        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // Also visit associated constants inside impl blocks
        for item in &node.items {
            if let syn::ImplItem::Const(const_item) = item {
                let name = const_item.ident.to_string();
                if name.ends_with("_FNS") {
                    let state_name = name.trim_end_matches("_FNS");
                    self.current_state_fns = Some(state_name.to_string());
                    self.extract_state_fns(&const_item.expr);
                    self.current_state_fns = None;
                }
            }
        }

        syn::visit::visit_item_impl(self, node);
    }
}

impl BloxExtractor {
    fn extract_state(&mut self, variant: &Variant) {
        let name = variant.ident.to_string();

        let is_composite = variant
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("composite"));
        let parent = variant.attrs.iter().find_map(|attr| {
            if attr.path().is_ident("parent") {
                attr.parse_args::<Ident>().ok().map(|i| i.to_string())
            } else {
                None
            }
        });

        let kind = if is_composite {
            StateKind::Composite
        } else {
            StateKind::Leaf
        };

        // Don't duplicate
        if !self.spec.states.iter().any(|s| s.name == name) {
            self.spec.states.push(State {
                name,
                kind,
                parent,
                description: String::new(),
                depth: 0,
            });
        }
    }

    fn resolve_state_name(&self, fns_name: &str) -> String {
        // Try to match the UPPER_SNAKE_CASE _FNS suffix to a PascalCase state name
        let candidates: Vec<&str> = self.spec.states.iter().map(|s| s.name.as_str()).collect();
        let fns_lower = fns_name.to_lowercase();

        for candidate in &candidates {
            if candidate.to_lowercase() == fns_lower {
                return candidate.to_string();
            }
        }

        // Fallback: try to convert the _FNS name to PascalCase
        // e.g., "ACTIVE" -> "Active"
        fns_name
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        first.to_uppercase().collect::<String>()
                            + chars.as_str().to_lowercase().as_str()
                    }
                }
            })
            .collect()
    }

    fn extract_state_fns(&mut self, expr: &Expr) {
        let state_name = match &self.current_state_fns {
            Some(n) => self.resolve_state_name(n),
            None => return,
        };

        // The expr should be a struct literal: StateFns { on_entry: &[...], on_exit: &[...], transitions: transitions![...] }
        if let Expr::Struct(expr_struct) = expr {
            for field in &expr_struct.fields {
                let member_name = match &field.member {
                    syn::Member::Named(ident) => ident.to_string(),
                    _ => continue,
                };

                match member_name.as_str() {
                    "on_entry" => {
                        if let Ok(actions) = parse_action_array(&field.expr) {
                            let entry_exit = self
                                .spec
                                .entry_exit
                                .entry(state_name.clone())
                                .or_insert_with(|| EntryExit {
                                    on_entry: Vec::new(),
                                    on_exit: Vec::new(),
                                });
                            entry_exit.on_entry = actions;
                        }
                    }
                    "on_exit" => {
                        if let Ok(actions) = parse_action_array(&field.expr) {
                            let entry_exit = self
                                .spec
                                .entry_exit
                                .entry(state_name.clone())
                                .or_insert_with(|| EntryExit {
                                    on_entry: Vec::new(),
                                    on_exit: Vec::new(),
                                });
                            entry_exit.on_exit = actions;
                        }
                    }
                    "transitions" => {
                        self.extract_transitions(&field.expr, &state_name);
                    }
                    _ => {}
                }
            }
        }
    }

    fn extract_transitions(&mut self, expr: &Expr, state_name: &str) {
        match expr {
            Expr::Macro(mac) => {
                if mac.mac.path.is_ident("transitions")
                    || mac
                        .mac
                        .path
                        .get_ident()
                        .map(|i| i.to_string() == "transitions")
                        .unwrap_or(false)
                {
                    let tokens = mac.mac.tokens.clone();
                    if let Ok(rules) = parse_transition_rules(&tokens) {
                        for rule in rules {
                            self.add_handler(state_name, rule);
                        }
                    }
                }
            }
            Expr::Reference(expr_ref) => {
                // Handle &[...] array of rules (for manually constructed transitions)
                self.extract_transitions(&expr_ref.expr, state_name);
            }
            Expr::Array(_expr_array) => {
                // Empty array: &[]
            }
            _ => {}
        }
    }

    fn add_handler(&mut self, state_name: &str, rule: TransitionRule) {
        let full_event = rule.event_pattern.clone();
        let (message_set, variant) = parse_event_pattern(&rule.event_pattern);

        // Add event if not already present
        if !self.spec.events.iter().any(|e| e.full_name == full_event) {
            self.spec.events.push(Event {
                message_set: message_set.clone(),
                variant: variant.clone(),
                full_name: full_event.clone(),
            });
        }

        // Determine target
        let target = if rule.target.is_empty() || rule.target == "stay" {
            Target::Stay
        } else if rule.target == "reset" || rule.target == "Reset" {
            Target::Reset
        } else {
            Target::Transition(rule.target.clone())
        };

        let label = if rule.actions.is_empty() {
            target.display()
        } else {
            let action_label = if rule.actions.len() == 1 {
                rule.actions[0].clone()
            } else {
                format!("{} actions", rule.actions.len())
            };
            format!("{} → {}", action_label, target.display())
        };

        let guard = Guard {
            description: rule.guard_description.clone(),
            raw: rule.guard_raw.clone(),
            branches: rule.guard_branches.clone(),
        };

        self.spec.handlers.push(Handler {
            state: state_name.to_string(),
            event: full_event,
            label,
            actions: rule.actions,
            guard,
            target,
            source: HandlerSource::Explicit,
            on_entry: Vec::new(),
            on_exit: Vec::new(),
        });
    }
}

#[derive(Debug)]
struct TransitionRule {
    event_pattern: String,
    actions: Vec<String>,
    guard_description: String,
    guard_raw: String,
    guard_branches: Vec<GuardBranch>,
    target: String,
}

fn parse_action_array(expr: &Expr) -> Result<Vec<String>, ()> {
    match expr {
        Expr::Reference(expr_ref) => parse_action_array(&expr_ref.expr),
        Expr::Array(ExprArray { elems, .. }) => {
            let mut actions = Vec::new();
            for elem in elems {
                if let Expr::Path(path) = elem {
                    actions.push(
                        path.path
                            .segments
                            .last()
                            .map(|s| s.ident.to_string())
                            .unwrap_or_default(),
                    );
                } else if let Expr::Call(call) = elem {
                    if let Expr::Path(path) = &*call.func {
                        actions.push(
                            path.path
                                .segments
                                .last()
                                .map(|s| s.ident.to_string())
                                .unwrap_or_default(),
                        );
                    }
                }
            }
            Ok(actions)
        }
        _ => Ok(Vec::new()),
    }
}

fn parse_transition_rules(tokens: &TokenStream) -> Result<Vec<TransitionRule>, String> {
    let mut rules = Vec::new();
    let mut arm_tokens = Vec::new();

    for tt in tokens.clone().into_iter() {
        match &tt {
            TokenTree::Punct(p) if p.as_char() == ',' => {
                if !arm_tokens.is_empty() {
                    let arm_stream: TokenStream = arm_tokens.drain(..).collect();
                    if let Ok(rule) = parse_transition_arm(arm_stream) {
                        rules.push(rule);
                    }
                }
            }
            TokenTree::Group(_g) => {
                // Groups are opaque at this level of token stream iteration.
                // The comma we want to split on is always a top-level punct token.
                arm_tokens.push(tt.clone());
            }
            _ => arm_tokens.push(tt.clone()),
        }
    }

    // Last arm (no trailing comma)
    if !arm_tokens.is_empty() {
        let arm_stream: TokenStream = arm_tokens.drain(..).collect();
        if let Ok(rule) = parse_transition_arm(arm_stream) {
            rules.push(rule);
        }
    }

    Ok(rules)
}

fn parse_transition_arm(tokens: TokenStream) -> Result<TransitionRule, String> {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();

    // Find the => separator
    let mut arrow_pos = None;
    for (i, tt) in tokens.iter().enumerate() {
        if let TokenTree::Punct(p) = tt {
            if p.as_char() == '=' && i + 1 < tokens.len() {
                if let TokenTree::Punct(p2) = &tokens[i + 1] {
                    if p2.as_char() == '>' {
                        arrow_pos = Some(i);
                        break;
                    }
                }
            }
        }
    }

    let arrow_pos = arrow_pos.ok_or("No => found in transition arm")?;

    // Pattern is everything before =>
    let pattern_tokens: TokenStream = tokens[..arrow_pos].iter().cloned().collect();
    let pattern = pattern_tokens.to_string().trim().to_string();

    // Body is everything after => (skip the >)
    let body_tokens: TokenStream = tokens[arrow_pos + 2..].iter().cloned().collect();

    // Check if body is just "stay" or "transition StateName"
    let body_str = body_tokens.to_string().trim().to_string();

    if body_str == "stay" || body_str.starts_with("stay ") {
        return Ok(TransitionRule {
            event_pattern: pattern,
            actions: Vec::new(),
            guard_description: "Stay".to_string(),
            guard_raw: body_str.clone(),
            guard_branches: vec![GuardBranch {
                condition: "_".to_string(),
                target: Target::Stay,
            }],
            target: "stay".to_string(),
        });
    }

    if body_str.starts_with("transition ") {
        let target = body_str
            .trim_start_matches("transition ")
            .trim()
            .to_string();
        return Ok(TransitionRule {
            event_pattern: pattern,
            actions: Vec::new(),
            guard_description: format!("Transition({})", target),
            guard_raw: body_str.clone(),
            guard_branches: vec![GuardBranch {
                condition: "_".to_string(),
                target: Target::Transition(target.clone()),
            }],
            target,
        });
    }

    // Parse full body: { actions [...] guard(ctx, results) { ... } }
    let mut actions = Vec::new();
    let mut guard_desc = String::new();
    let mut guard_branches = Vec::new();
    let mut target = String::new();

    // Simple token-based parsing of the body
    let mut in_actions = false;
    let mut in_guard = false;
    let mut in_guard_chain = false;
    let mut depth: usize = 0;
    let mut current_token = String::new();

    for tt in body_tokens.into_iter() {
        match tt {
            TokenTree::Ident(ident) => {
                let s = ident.to_string();
                if s == "actions" && !in_actions && !in_guard {
                    in_actions = true;
                } else if s == "guard" && !in_guard {
                    in_guard = true;
                } else if in_actions && depth == 1 {
                    if s == "Self" {
                        current_token = "Self::".to_string();
                    } else {
                        if !current_token.is_empty() {
                            current_token.push_str(&s);
                        } else {
                            current_token = s;
                        }
                    }
                } else if in_guard_chain && depth == 1 {
                    // Inside guard chain body
                    if s == "stay" && !target.is_empty() {
                        // This is a target in guard chain
                    }
                } else if in_guard {
                    // skip guard args
                }
            }
            TokenTree::Punct(p) => {
                let ch = p.as_char();
                if in_actions && depth == 1 {
                    if ch == ':' && current_token == "Self" {
                        current_token.push_str("::");
                    } else if ch == ',' && !current_token.is_empty() {
                        actions.push(current_token.trim().to_string());
                        current_token.clear();
                    } else if ch == ']' {
                        if !current_token.is_empty() {
                            actions.push(current_token.trim().to_string());
                            current_token.clear();
                        }
                        in_actions = false;
                    }
                } else if in_guard {
                    if ch == ')' && depth == 1 {
                        // End of guard args
                    }
                } else if in_guard_chain {
                    if ch == '=' && depth == 1 {
                        // => in guard chain
                    } else if ch == '>' {
                        // part of =>
                    }
                }
            }
            TokenTree::Group(g) => {
                match g.delimiter() {
                    Delimiter::Bracket => {
                        if in_actions {
                            // Parse the group content as actions
                            let group_str = g.stream().to_string();
                            for action in group_str.split(',') {
                                let a = action.trim().to_string();
                                if !a.is_empty() {
                                    actions.push(a);
                                }
                            }
                            in_actions = false;
                        }
                    }
                    Delimiter::Brace => {
                        if in_guard {
                            depth += 1;
                            if depth == 1 {
                                in_guard_chain = true;
                                guard_desc = g.stream().to_string();
                                // Parse guard branches
                                guard_branches = parse_guard_chain(&g.stream());
                            }
                        }
                    }
                    _ => {}
                }
            }
            TokenTree::Literal(lit) => {
                if in_guard_chain && depth == 1 {
                    let s = lit.to_string();
                    if target.is_empty() {
                        target = s.trim().to_string();
                    }
                }
            }
        }
    }

    if target.is_empty() {
        target = "stay".to_string();
    }

    Ok(TransitionRule {
        event_pattern: pattern,
        actions,
        guard_description: if guard_desc.is_empty() {
            target.clone()
        } else {
            guard_desc
        },
        guard_raw: body_str,
        guard_branches,
        target,
    })
}

fn parse_guard_chain(tokens: &TokenStream) -> Vec<GuardBranch> {
    let mut branches = Vec::new();
    let tokens: Vec<TokenTree> = tokens.clone().into_iter().collect();

    let mut i = 0;
    while i < tokens.len() {
        // Look for pattern => target,
        let mut cond_tokens = Vec::new();
        while i < tokens.len() {
            if let TokenTree::Punct(p) = &tokens[i] {
                if p.as_char() == '=' {
                    // Check if next is >
                    if i + 1 < tokens.len() {
                        if let TokenTree::Punct(p2) = &tokens[i + 1] {
                            if p2.as_char() == '>' {
                                break;
                            }
                        }
                    }
                }
            }
            cond_tokens.push(tokens[i].clone());
            i += 1;
        }

        if i >= tokens.len() {
            break;
        }

        // Skip =>
        i += 2;

        // Read target
        let mut target_tokens = Vec::new();
        while i < tokens.len() {
            if let TokenTree::Punct(p) = &tokens[i] {
                if p.as_char() == ',' {
                    i += 1;
                    break;
                }
            }
            target_tokens.push(tokens[i].clone());
            i += 1;
        }

        let condition = cond_tokens.into_iter().collect::<TokenStream>().to_string();
        let target_str = target_tokens
            .into_iter()
            .collect::<TokenStream>()
            .to_string();

        let target = if target_str.trim() == "stay" {
            Target::Stay
        } else if target_str.trim().starts_with("Transition") {
            if let Some(inner) = extract_transition_target(&target_str) {
                Target::Transition(inner)
            } else {
                Target::Stay
            }
        } else {
            Target::Transition(target_str.trim().to_string())
        };

        branches.push(GuardBranch {
            condition: condition.trim().to_string(),
            target,
        });
    }

    branches
}

fn extract_transition_target(s: &str) -> Option<String> {
    // Extract StateName from "Transition(StateName)" or "Transition(StateName { ... })"
    let s = s.trim();
    if s.starts_with("Transition(") {
        let inner = &s["Transition(".len()..];
        if let Some(end) = inner.rfind(')') {
            let state = inner[..end].trim().to_string();
            // Take just the first token (state name) if there are braces
            Some(state.split_whitespace().next()?.to_string())
        } else {
            None
        }
    } else {
        Some(s.to_string())
    }
}

fn parse_event_pattern(pattern: &str) -> (String, String) {
    let pattern = pattern.trim();
    // Remove trailing parenthetical content like (_)
    let clean = if let Some(pos) = pattern.find('(') {
        &pattern[..pos]
    } else {
        pattern
    };

    if let Some(pos) = clean.find("::") {
        let message_set = clean[..pos].trim().to_string();
        let variant = clean[pos + 2..].trim().to_string();
        (message_set, variant)
    } else {
        ("Unknown".to_string(), clean.to_string())
    }
}

/// Parse a source file and extract blox metadata.
pub fn parse_blox_file(name: &str, crate_path: &str, source: &str) -> Result<BloxSpec, String> {
    let file = syn::parse_file(source).map_err(|e| e.to_string())?;

    let mut extractor = BloxExtractor::new(name, crate_path);
    syn::visit::visit_file(&mut extractor, &file);

    // Compute hierarchy depths
    compute_hierarchy(&mut extractor.spec.states);

    // Fill inherited handlers for composite state children
    fill_inherited_handlers(&mut extractor.spec.handlers, &extractor.spec.states);

    // Fill dropped handlers
    fill_dropped_handlers(
        &mut extractor.spec.handlers,
        &extractor.spec.states,
        &extractor.spec.events,
    );

    // Build message sets from events
    let mut sets: HashMap<String, Vec<String>> = HashMap::new();
    for event in &extractor.spec.events {
        sets.entry(event.message_set.clone())
            .or_default()
            .push(event.variant.clone());
    }
    extractor.spec.message_sets = sets
        .into_iter()
        .map(|(name, variants)| MessageSet { name, variants })
        .collect();

    Ok(extractor.spec)
}

fn compute_hierarchy(states: &mut Vec<State>) {
    for state in states.iter_mut() {
        if state.parent.is_some() {
            state.depth = 1;
        }
    }
}

fn fill_inherited_handlers(handlers: &mut Vec<Handler>, states: &[State]) {
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

fn fill_dropped_handlers(handlers: &mut Vec<Handler>, states: &[State], events: &[Event]) {
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
