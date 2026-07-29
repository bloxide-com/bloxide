// Copyright 2025 Bloxide, all rights reserved
//! `cargo blox lint` — friendly TOML validation for blox.toml files (issues
//! #114, #122).
//!
//! Codegen validates some of these errors, but they surface as codegen
//! failures or Rust compile errors — not actionable messages. Lint collects
//! diagnostics across every blox.toml in the workspace and reports them with
//! did-you-mean suggestions.
//!
//! Checks (errors):
//! - transition `state` / `target` / guard `target` / entry / exit / parent
//!   referencing undeclared states
//! - duplicate state names and duplicate transitions (same state + event)
//! - event patterns referencing unknown variants of KNOWN enums (the blox's
//!   own event enum, workspace message enums, framework enums)
//! - `Self::` actions not declared in `[[context.actions]]`; bare action
//!   functions missing from `spec_imports`
//! - guard conditions referencing undeclared `ctx.<field>` fields
//!
//! Warnings (do not fail the run):
//! - unreachable states (nothing targets them, not initial, not error)
//! - states with no outgoing transitions (events bubble to parents)

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bloxide_codegen::schema::{BloxConfig, TopologyConfig};
use walkdir::WalkDir;

const TARGET_KEYWORDS: [&str; 5] = ["stay", "reset", "stop", "done", "fail"];

enum Level {
    Error,
    Warning,
}

struct Diagnostic {
    path: PathBuf,
    level: Level,
    message: String,
}

impl Diagnostic {
    fn error(path: &Path, message: String) -> Self {
        Self {
            path: path.to_path_buf(),
            level: Level::Error,
            message,
        }
    }
    fn warning(path: &Path, message: String) -> Self {
        Self {
            path: path.to_path_buf(),
            level: Level::Warning,
            message,
        }
    }
}

pub fn lint() -> anyhow::Result<()> {
    let files = discover_blox_tomls();
    if files.is_empty() {
        println!("bloxide: no blox.toml files found");
        return Ok(());
    }

    // Phase 1: parse every file; build the message enum registry.
    let mut configs: Vec<(PathBuf, BloxConfig)> = Vec::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    for path in &files {
        let content = std::fs::read_to_string(path)?;
        match toml::from_str::<BloxConfig>(&content) {
            Ok(config) => configs.push((path.clone(), config)),
            Err(e) => diags.push(Diagnostic::error(path, format!("TOML parse error: {}", e))),
        }
    }

    let message_enums = build_message_registry(&configs);

    // Phase 2: per-file checks.
    for (path, config) in &configs {
        if let Some(topology) = &config.topology {
            lint_topology(path, config, topology, &message_enums, &mut diags);
        }
    }

    // Report.
    let mut errors = 0;
    let mut warnings = 0;
    for d in &diags {
        match d.level {
            Level::Error => {
                errors += 1;
                eprintln!("error: {}", d.message);
            }
            Level::Warning => {
                warnings += 1;
                println!("warning: {}", d.message);
            }
        }
        eprintln!("  --> {}", d.path.display());
    }

    println!();
    println!(
        "bloxide lint: {} error(s), {} warning(s) across {} blox.toml files",
        errors,
        warnings,
        configs.len()
    );
    if errors > 0 {
        anyhow::bail!("{} lint error(s)", errors);
    }
    Ok(())
}

fn discover_blox_tomls() -> Vec<PathBuf> {
    WalkDir::new(".")
        .max_depth(5)
        .into_iter()
        .filter_entry(|e| e.file_name() != "target")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() == "blox.toml")
        .map(|e| e.into_path())
        .collect()
}

/// Enum name → variant names, from every workspace blox.toml `[[messages]]`
/// section plus the stable framework enums.
fn build_message_registry(configs: &[(PathBuf, BloxConfig)]) -> BTreeMap<String, BTreeSet<String>> {
    let mut registry: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (_, config) in configs {
        if let Some(messages) = &config.messages {
            for en in messages {
                registry.insert(
                    en.name.clone(),
                    en.variants.iter().map(|v| v.name.clone()).collect(),
                );
            }
        }
    }
    // Framework enums (stable platform message sets — spec 20).
    for (name, variants) in [
        ("LifecycleCommand", vec!["Start", "Reset", "Stop", "Ping"]),
        (
            "ChildLifecycleEvent",
            vec![
                "Started", "Stopped", "Done", "Failed", "Aborted", "Killed", "Alive",
            ],
        ),
        ("AbortCommand", vec!["Abort"]),
        ("TimerCommand", vec!["Set", "Cancel"]),
        ("PeerCtrl", vec!["AddPeer", "RemovePeer"]),
        (
            "ChildCtrl",
            vec!["RegisterChild", "RegisterDynamicChild", "HealthCheckTick"],
        ),
    ] {
        registry.insert(
            name.to_string(),
            variants.iter().map(|s| s.to_string()).collect(),
        );
    }
    registry
}

fn lint_topology(
    path: &Path,
    config: &BloxConfig,
    topology: &TopologyConfig,
    message_enums: &BTreeMap<String, BTreeSet<String>>,
    diags: &mut Vec<Diagnostic>,
) {
    let state_names: BTreeSet<String> = topology.states.iter().map(|s| s.name.clone()).collect();

    // Duplicate state names.
    let mut seen = BTreeSet::new();
    for s in &topology.states {
        if !seen.insert(&s.name) {
            diags.push(Diagnostic::error(
                path,
                format!("duplicate state name \"{}\"", s.name),
            ));
        }
    }

    // Parent references.
    for s in &topology.states {
        if let Some(parent) = &s.parent {
            if !state_names.contains(parent) {
                diags.push(Diagnostic::error(
                    path,
                    format!(
                        "state \"{}\" has unknown parent \"{}\"{}",
                        s.name,
                        parent,
                        did_you_mean(parent, &state_names)
                    ),
                ));
            }
        }
    }

    // The blox's own event enum variants: Lifecycle + mailbox variants.
    let mut event_variants: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if let Some(event) = &config.event {
        let mut variants: BTreeSet<String> =
            event.mailboxes.iter().map(|m| m.variant.clone()).collect();
        variants.insert("Lifecycle".to_string());
        event_variants.insert(event.name.clone(), variants);
    }

    // Context field names for guard ctx.<field> checks.
    let ctx_fields = collect_ctx_fields(config);

    // Declared action names for Self:: checks.
    let action_names: BTreeSet<String> = config
        .context
        .as_ref()
        .map(|c| c.actions.iter().map(|a| a.name.clone()).collect())
        .unwrap_or_default();

    // Transitions.
    let mut transition_keys = BTreeSet::new();
    for t in &topology.transitions {
        if !state_names.contains(&t.state) {
            diags.push(Diagnostic::error(
                path,
                format!(
                    "transition references unknown state \"{}\"{}",
                    t.state,
                    did_you_mean(&t.state, &state_names)
                ),
            ));
        }
        check_target(
            path,
            &format!("transition in state \"{}\"", t.state),
            &t.target,
            &state_names,
            diags,
        );
        if !transition_keys.insert((t.state.clone(), t.event.clone())) {
            diags.push(Diagnostic::error(
                path,
                format!(
                    "duplicate transition in state \"{}\" for event pattern `{}`",
                    t.state, t.event
                ),
            ));
        }
        check_event_pattern(path, &t.event, &event_variants, message_enums, diags);
        for g in &t.guards {
            check_target(
                path,
                &format!("guard in state \"{}\"", t.state),
                &g.target,
                &state_names,
                diags,
            );
            check_guard_condition(path, &t.state, &g.condition, &ctx_fields, diags);
        }
        for a in &t.actions {
            check_action_ref(path, a, &action_names, &topology.spec_imports, diags);
        }
    }

    // Entry / exit.
    for ee in topology.entry.iter().chain(topology.exit.iter()) {
        if !state_names.contains(&ee.state) {
            diags.push(Diagnostic::error(
                path,
                format!(
                    "entry/exit references unknown state \"{}\"{}",
                    ee.state,
                    did_you_mean(&ee.state, &state_names)
                ),
            ));
        }
        for a in &ee.actions {
            check_action_ref(path, a, &action_names, &topology.spec_imports, diags);
        }
    }

    // Unreachable / dead states (warnings).
    let initial = topology
        .states
        .iter()
        .find(|s| s.initial.unwrap_or(false))
        .or_else(|| {
            topology
                .states
                .iter()
                .find(|s| !s.composite.unwrap_or(false))
        });
    let targeted: BTreeSet<&str> = topology
        .transitions
        .iter()
        .flat_map(|t| {
            std::iter::once(t.target.as_str()).chain(t.guards.iter().map(|g| g.target.as_str()))
        })
        .filter(|t| !TARGET_KEYWORDS.contains(t))
        .collect();
    for s in &topology.states {
        let is_error = s.error.unwrap_or(false);
        let is_initial = initial.map(|i| i.name == s.name).unwrap_or(false);
        let is_composite = s.composite.unwrap_or(false);
        if !is_initial && !is_error && !is_composite && !targeted.contains(s.name.as_str()) {
            diags.push(Diagnostic::warning(
                path,
                format!(
                    "state \"{}\" is unreachable (no transition targets it)",
                    s.name
                ),
            ));
        }
        if !is_error && !is_composite && !topology.transitions.iter().any(|t| t.state == s.name) {
            diags.push(Diagnostic::warning(
                path,
                format!(
                    "state \"{}\" has no outgoing transitions (events bubble to parent)",
                    s.name
                ),
            ));
        }
    }
}

fn check_target(
    path: &Path,
    context: &str,
    target: &str,
    state_names: &BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if TARGET_KEYWORDS.contains(&target) || state_names.contains(target) {
        return;
    }
    diags.push(Diagnostic::error(
        path,
        format!(
            "{} references unknown target state \"{}\"{}",
            context,
            target,
            did_you_mean(target, state_names)
        ),
    ));
}

/// Scan an event pattern for `Ident::Ident` pairs and validate them against
/// known enums: the blox's own event enum first, then workspace message
/// enums and framework enums. Unknown enum names are skipped (they may be
/// context or payload types).
fn check_event_pattern(
    path: &Path,
    pattern: &str,
    event_variants: &BTreeMap<String, BTreeSet<String>>,
    message_enums: &BTreeMap<String, BTreeSet<String>>,
    diags: &mut Vec<Diagnostic>,
) {
    for (enum_name, variant) in scan_path_pairs(pattern) {
        if let Some(variants) = event_variants.get(&enum_name) {
            if !variants.contains(&variant) {
                diags.push(Diagnostic::error(
                    path,
                    format!(
                        "event pattern references unknown variant `{}::{}`{}",
                        enum_name,
                        variant,
                        did_you_mean(&variant, variants)
                    ),
                ));
            }
        } else if let Some(variants) = message_enums.get(&enum_name) {
            if !variants.contains(&variant) {
                diags.push(Diagnostic::error(
                    path,
                    format!(
                        "event pattern references unknown variant `{}::{}`{}",
                        enum_name,
                        variant,
                        did_you_mean(&variant, variants)
                    ),
                ));
            }
        }
    }
}

/// Extract `Ident::Ident` pairs from a match pattern (the leftmost two
/// segments of each path, e.g. `PingPongMsg::Ping`, `SupervisorEvent::Child`).
fn scan_path_pairs(pattern: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = pattern.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while i < bytes.len() {
        if is_ident(bytes[i]) && (i == 0 || !is_ident(bytes[i - 1])) {
            let start = i;
            while i < bytes.len() && is_ident(bytes[i]) {
                i += 1;
            }
            let first = &pattern[start..i];
            // Expect `::` followed by another ident, but not `:::`
            if i + 1 < bytes.len() && &pattern[i..i + 2] == "::" {
                let mut j = i + 2;
                if j < bytes.len() && is_ident(bytes[j]) {
                    let vstart = j;
                    while j < bytes.len() && is_ident(bytes[j]) {
                        j += 1;
                    }
                    let second = &pattern[vstart..j];
                    // Skip generic type params (e.g. Option::None stays, but
                    // Envelope::new etc. — we only validate known enums anyway)
                    out.push((first.to_string(), second.to_string()));
                    i = j;
                    continue;
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

fn check_guard_condition(
    path: &Path,
    state: &str,
    condition: &str,
    ctx_fields: &BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let bytes = condition.as_bytes();
    let mut i = 0;
    while let Some(pos) = condition[i..].find("ctx.") {
        let start = i + pos + 4;
        let rest = &condition[start..];
        let field: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if field.is_empty() {
            i = start;
            continue;
        }
        // Method calls (e.g. ctx.all_children_stopped()) are allowed — only
        // bare field accesses are validated against declared fields.
        let after = rest[field.len()..].trim_start();
        if !after.starts_with('(') && !ctx_fields.contains(&field) {
            diags.push(Diagnostic::error(
                path,
                format!(
                    "guard in state \"{}\" references unknown context field `ctx.{}`{}",
                    state,
                    field,
                    did_you_mean(&field, ctx_fields)
                ),
            ));
        }
        i = start + field.len();
        let _ = bytes;
    }
}

fn check_action_ref(
    path: &Path,
    action: &str,
    action_names: &BTreeSet<String>,
    spec_imports: &[String],
    diags: &mut Vec<Diagnostic>,
) {
    if let Some(name) = action.strip_prefix("Self::") {
        if !action_names.contains(name) {
            diags.push(Diagnostic::error(
                path,
                format!(
                    "action `{}` is not declared in [[context.actions]]{}",
                    action,
                    did_you_mean(name, action_names)
                ),
            ));
        }
        return;
    }
    // Bare function path: its last segment must come from spec_imports.
    let last = action.rsplit("::").next().unwrap_or(action);
    let imported = spec_imports.iter().any(|imp| imp.contains(last));
    if !imported {
        diags.push(Diagnostic::error(
            path,
            format!(
                "action `{}` does not appear in any spec_imports entry",
                action
            ),
        ));
    }
}

/// Context field names: auto `self_id`, `[[context.uses]]` single fields and
/// `fields[]` sub-entries, and `[[context.fields]]` state fields.
fn collect_ctx_fields(config: &BloxConfig) -> BTreeSet<String> {
    let mut fields = BTreeSet::from(["self_id".to_string()]);
    if let Some(ctx) = &config.context {
        for u in &ctx.uses {
            if let Some(f) = &u.field {
                fields.insert(f.clone());
            }
            for f in &u.fields {
                fields.insert(f.name.clone());
            }
        }
        for f in &ctx.fields {
            fields.insert(f.name.clone());
        }
    }
    fields
}

/// Levenshtein-based suggestion: returns ` — did you mean "X"?` when a close
/// candidate exists.
fn did_you_mean(input: &str, candidates: &BTreeSet<String>) -> String {
    let mut best: Option<(&String, usize)> = None;
    for c in candidates {
        let d = levenshtein(input, c);
        if best.as_ref().map(|(_, bd)| d < *bd).unwrap_or(true) {
            best = Some((c, d));
        }
    }
    match best {
        Some((c, d)) if d <= 3 && d <= input.len().max(c.len()) / 2 => {
            format!(" — did you mean \"{}\"?", c)
        }
        _ => String::new(),
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
