// Copyright 2025 Bloxide, all rights reserved
//! Scaffold a new impl crate for a blox.
//!
//! `cargo blox new-impl <name> --blox <blox-name>` reads the blox's
//! `blox.toml`, finds `[[context.actions]]` entries with `impl_required = true`,
//! and generates a crate with:
//!   - `Cargo.toml` with deps on the blox crate, message crates, and
//!     context/service crates referenced by the action signatures.
//!   - `src/lib.rs` with function stubs matching the declared signatures
//!     (parameters derived from `fields` + `event_payload`).
//!   - Registration in the workspace `Cargo.toml` (members + deps).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use bloxide_codegen::schema::{BloxConfig, ContextActionConfig, ContextConfig};

use crate::toml_helpers::blox_toml_path_for_blox;
use crate::utils::{to_camel_case, update_workspace_cargo_toml, WorkspaceAddition};

/// Run the `new-impl` command.
///
/// Creates `crates/impl/<name>/` with a `Cargo.toml` and `src/lib.rs`
/// containing function stubs for all `impl_required = true` actions
/// declared in the specified blox's `blox.toml`.
pub fn new_impl(name: &str, blox_name: &str) -> Result<()> {
    // `name` is used as-is for the package name (hyphens preserved).
    // `name_snake` is used for the directory name (underscores).
    let name_snake = name.to_lowercase().replace('-', "_");
    let blox_snake = blox_name.to_lowercase().replace('-', "_");

    // ── Load and parse the blox.toml ────────────────────────────────────────
    let blox_toml_path = blox_toml_path_for_blox(&blox_snake);
    if !blox_toml_path.exists() {
        bail!(
            "blox.toml not found for blox '{}' at {}",
            blox_snake,
            blox_toml_path.display()
        );
    }

    let toml_content = fs::read_to_string(&blox_toml_path)
        .with_context(|| format!("failed to read {}", blox_toml_path.display()))?;
    let blox_config: BloxConfig = toml::from_str(&toml_content)
        .with_context(|| format!("failed to parse {}", blox_toml_path.display()))?;

    let context = blox_config.context.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "blox '{}' has no [context] section — no actions to implement",
            blox_snake
        )
    })?;

    // ── Collect impl_required actions ───────────────────────────────────────
    let impl_actions: Vec<&ContextActionConfig> =
        context.actions.iter().filter(|a| a.impl_required).collect();

    if impl_actions.is_empty() {
        bail!(
            "blox '{}' has no [[context.actions]] with impl_required = true",
            blox_snake
        );
    }

    // ── Build field type map (field name → Rust type) ───────────────────────
    let field_types = build_field_type_map(context);

    // ── Build message variant map (snake_case → TypeName) ───────────────────
    let message_variants = build_message_variant_map(&blox_config);

    // ── Build payload type map (snake_case → full type with generics) ───────
    let payload_types = build_payload_type_map(&blox_config);

    // ── Determine the message crate names (for payload types) ───────────────
    let message_crates = collect_message_crate_names(&blox_config);

    // ── Determine all crate dependencies ────────────────────────────────────
    let blox_crate_name = format!("{}-blox", blox_snake);
    let mut dep_crates: BTreeSet<String> = BTreeSet::new();
    dep_crates.insert("bloxide-core".to_string());

    // Always depend on the blox crate (for context types).
    dep_crates.insert(blox_crate_name.clone());

    // Depend on all message crates referenced in the event.
    for mc in &message_crates {
        dep_crates.insert(mc.clone());
    }

    // Depend on context/service crates from [[context.uses]].
    for uses in &context.uses {
        let hyphen_crate = uses.crate_name.replace('_', "-");
        dep_crates.insert(hyphen_crate);
    }

    // Scan context.imports and context.feature_imports for additional crate names.
    // These are raw `use` statements like "pool_messages::{PoolMsg, WorkerMsg}"
    // or "bloxide_spawn::SpawnFn".
    for import in context.imports.iter().chain(context.feature_imports.iter()) {
        if let Some(crate_part) = import.split("::").next() {
            let trimmed = crate_part.trim();
            // Skip "alloc" — it's a core crate, not a workspace dep.
            if trimmed != "alloc" {
                dep_crates.insert(trimmed.replace('_', "-"));
            }
        }
    }

    // ── Generate the crate ──────────────────────────────────────────────────
    let crate_dir = Path::new("crates/impl").join(&name_snake);
    let src_dir = crate_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    // Cargo.toml — use `name` (hyphens) for the package name.
    let cargo_toml = generate_cargo_toml(name, &dep_crates, &impl_actions);
    fs::write(crate_dir.join("Cargo.toml"), cargo_toml)?;

    // src/lib.rs
    let lib_rs = generate_lib_rs(
        &blox_snake,
        &impl_actions,
        &field_types,
        &message_variants,
        &payload_types,
        &message_crates,
        context,
    );
    fs::write(src_dir.join("lib.rs"), lib_rs)?;

    // ── Register in workspace Cargo.toml ────────────────────────────────────
    let member_path = format!("crates/impl/{}", name_snake);
    let dep_name = format!("{}-impl", name);
    let dep_toml_line = format!(
        r#"{} = {{ path = "crates/impl/{}" }}"#,
        dep_name, name_snake
    );
    update_workspace_cargo_toml(&[
        WorkspaceAddition::Member(member_path),
        WorkspaceAddition::Dependency {
            name: dep_name.clone(),
            toml_line: dep_toml_line,
        },
    ])?;

    println!(
        "\nScaffolded impl crate '{}' for blox '{}'",
        name, blox_name
    );
    println!("Created: {}", crate_dir.display());
    println!("\nNext steps:");
    println!(
        "  1. Edit crates/impl/{}/src/lib.rs to implement the action functions",
        name_snake
    );
    println!(
        "  2. Reference this crate in your system.toml: impl_crate = \"{}\"",
        dep_name
    );
    println!("  3. Run `cargo blox generate` to regenerate wiring code");

    Ok(())
}

// ── Field type resolution ────────────────────────────────────────────────────

/// Build a map from context field name → Rust type string.
///
/// Collects fields from:
///   - `self_id` (auto-injected as `ActorId`)
///   - `[[context.uses]]` single-field accessors (field + field_type)
///   - `[[context.uses.fields]]` multi-field entries (name + ty)
///   - `[[context.fields]]` direct context fields (name + type)
fn build_field_type_map(context: &ContextConfig) -> Vec<FieldInfo> {
    let mut fields = Vec::new();

    // self_id is always auto-injected by the codegen.
    fields.push(FieldInfo {
        name: "self_id".to_string(),
        ty: "bloxide_core::ActorId".to_string(),
    });

    // [[context.uses]] single-field accessors
    for uses in &context.uses {
        if let (Some(field), Some(ty)) = (&uses.field, &uses.field_type) {
            fields.push(FieldInfo {
                name: field.clone(),
                ty: ty.clone(),
            });
        }
        // [[context.uses.fields]] multi-field
        for sub in &uses.fields {
            fields.push(FieldInfo {
                name: sub.name.clone(),
                ty: sub.ty.clone(),
            });
        }
    }

    // [[context.fields]]
    for field in &context.fields {
        fields.push(FieldInfo {
            name: field.name.clone(),
            ty: field.r#type.clone(),
        });
    }

    fields
}

/// Information about a context field.
struct FieldInfo {
    name: String,
    ty: String,
}

// ── Message variant resolution ───────────────────────────────────────────────

/// Build a map from snake_case variant name → TypeName.
///
/// Scans all `[[messages]]` sections in the blox.toml for their variants,
/// converting variant names to snake_case for lookup by `event_payload`.
fn build_message_variant_map(blox_config: &BloxConfig) -> Vec<MessageVariantInfo> {
    let mut variants = Vec::new();

    if let Some(messages) = &blox_config.messages {
        for msg_enum in messages {
            for variant in &msg_enum.variants {
                variants.push(MessageVariantInfo {
                    snake: to_snake_case_name(&variant.name),
                    type_name: variant.name.clone(),
                });
            }
        }
    }

    variants
}

/// Information about a message variant for payload type resolution.
struct MessageVariantInfo {
    snake: String,
    type_name: String,
}

/// Convert a PascalCase name to snake_case.
fn to_snake_case_name(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

// ── Message crate name collection ────────────────────────────────────────────

/// Collect the message crate names referenced in the event mailboxes.
///
/// e.g. `message_path = "pool_messages::PoolMsg"` → `"pool-messages"`
fn collect_message_crate_names(blox_config: &BloxConfig) -> Vec<String> {
    let mut crates = Vec::new();
    if let Some(event) = &blox_config.event {
        for mb in &event.mailboxes {
            if let Some(path) = &mb.message_path {
                // Extract the crate name (first segment before ::)
                if let Some(crate_part) = path.split("::").next() {
                    let hyphen = crate_part.replace('_', "-");
                    if !crates.contains(&hyphen) {
                        crates.push(hyphen);
                    }
                }
            }
        }
    }
    crates
}

/// Build a map from payload snake_case → full type (including generics, sans crate paths).
///
/// Scans the event mailboxes. For each mailbox, the `message` field gives the
/// short type name (e.g. "SpawnedWorker") and `message_path` gives the full
/// path with generics (e.g. "pool_messages::SpawnedWorker<bloxide_peers::PeerCtrl<pool_messages::WorkerMsg, R>, R>").
///
/// We strip all `crate_name::` prefixes to get just the type name with generics
/// (e.g. "SpawnedWorker<PeerCtrl<WorkerMsg, R>, R>"), since the impl crate
/// already imports all needed types via glob imports.
fn build_payload_type_map(blox_config: &BloxConfig) -> Vec<PayloadTypeInfo> {
    let mut entries = Vec::new();
    if let Some(event) = &blox_config.event {
        for mb in &event.mailboxes {
            let message_name = &mb.message;
            let snake = to_snake_case_name(message_name);
            let full_type = if let Some(path) = &mb.message_path {
                strip_crate_prefixes(path)
            } else {
                message_name.clone()
            };
            entries.push(PayloadTypeInfo { snake, full_type });
        }
    }
    entries
}

/// Strip all `crate_name::` prefixes from a type path, leaving just the
/// type names and generic args.
///
/// e.g. "pool_messages::SpawnedWorker<bloxide_peers::PeerCtrl<pool_messages::WorkerMsg, R>, R>"
///      → "SpawnedWorker<PeerCtrl<WorkerMsg, R>, R>"
fn strip_crate_prefixes(s: &str) -> String {
    // Remove all occurrences of `identifier::` that precede a type name.
    // We use a simple approach: replace `word::` with `` for all word:: prefixes.
    // This is safe because `::` only appears as a path separator.
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    let mut ident = String::new();

    while let Some(ch) = chars.next() {
        if ch.is_alphanumeric() || ch == '_' {
            ident.push(ch);
        } else if ch == ':' && chars.peek() == Some(&':') {
            // `ident::` — this is a crate prefix, skip it (consume the second `:`)
            chars.next();
            ident.clear();
        } else {
            // Non-ident, non-`::` char — flush the ident and the char
            if !ident.is_empty() {
                result.push_str(&ident);
                ident.clear();
            }
            result.push(ch);
        }
    }
    // Flush any remaining ident
    if !ident.is_empty() {
        result.push_str(&ident);
    }
    result
}

/// Information about a payload type for resolution.
struct PayloadTypeInfo {
    snake: String,
    full_type: String,
}

// ── Cargo.toml generation ────────────────────────────────────────────────────

/// Generate the Cargo.toml for the impl crate.
fn generate_cargo_toml(
    name: &str,
    dep_crates: &BTreeSet<String>,
    impl_actions: &[&ContextActionConfig],
) -> String {
    let mut deps = String::new();
    for dep in dep_crates {
        deps.push_str(&format!("{} = {{ workspace = true }}\n", dep));
    }

    // Collect features used by impl_required actions.
    let features: BTreeSet<String> = impl_actions
        .iter()
        .filter_map(|a| a.feature.clone())
        .collect();

    let features_section = if features.is_empty() {
        String::new()
    } else {
        let mut s = String::from("\n[features]\n");
        for f in &features {
            s.push_str(&format!("{} = []\n", f));
        }
        s
    };

    format!(
        r#"# Copyright 2025 Bloxide, all rights reserved
[package]
name = "{name}-impl"
version.workspace = true
edition.workspace = true
description = "Concrete action implementations for {name}"
publish = false

[dependencies]
{deps}{features_section}"#
    )
}

// ── lib.rs generation ────────────────────────────────────────────────────────

/// Generate the `src/lib.rs` with function stubs.
fn generate_lib_rs(
    blox_snake: &str,
    impl_actions: &[&ContextActionConfig],
    field_types: &[FieldInfo],
    message_variants: &[MessageVariantInfo],
    payload_types: &[PayloadTypeInfo],
    message_crates: &[String],
    context: &ContextConfig,
) -> String {
    let mut out = String::new();

    out.push_str("// Copyright 2025 Bloxide, all rights reserved\n");
    out.push_str(&format!(
        "//! Concrete action implementations for the `{}` blox.\n",
        blox_snake
    ));
    out.push_str("//!\n");
    out.push_str("//! This crate is the impl layer — it provides the free functions\n");
    out.push_str("//! referenced by `impl_required = true` actions in the blox's\n");
    out.push_str("//! `[[context.actions]]` declarations.\n\n");

    // Determine if we need alloc (for Vec fields).
    let needs_alloc = field_types.iter().any(|f| f.ty.contains("Vec<"));
    if needs_alloc {
        out.push_str("extern crate alloc;\n\n");
    }

    // ── Generate use statements ──────────────────────────────────────────────
    // We need to import all types referenced in the function signatures.
    // Strategy:
    //   1. Glob import from bloxide_core (brings in ActorRef, ActorId, etc.)
    //   2. Glob import from each message crate (brings in payload variants)
    //   3. The blox's context.imports (explicit types the blox author listed)
    //   4. Feature-gated context.feature_imports (behind cfg)
    //
    // Glob imports are the simplest way to ensure all types resolve without
    // having to trace every type reference in field_type strings.

    // bloxide_core glob — ActorRef, ActorId, BloxRuntime, etc.
    out.push_str("use bloxide_core::*;\n");

    // Message crate globs — payload variants (SpawnWorker, WorkDone, etc.)
    for mc in message_crates {
        out.push_str(&format!("use {}::*;\n", mc.replace('-', "_")));
    }

    // Non-feature imports from the blox's [context.imports]
    if !context.imports.is_empty() {
        for import in &context.imports {
            // Skip alloc::vec::Vec — it's covered by the extern crate + std.
            if import.contains("alloc::vec::Vec") {
                continue;
            }
            out.push_str(&format!("use {};\n", import));
        }
    }

    // Feature-gated imports from [context.feature_imports]
    if !context.feature_imports.is_empty() {
        if let Some(feature) = &context.feature {
            out.push_str(&format!("\n#[cfg(feature = \"{}\")]\n", feature));
            out.push_str("mod dynamic_imports {\n");
            for import in &context.feature_imports {
                out.push_str(&format!("    pub use {};\n", import));
            }
            out.push_str("}\n");
            out.push_str(&format!("#[cfg(feature = \"{}\")]\n", feature));
            out.push_str("use dynamic_imports::*;\n");
        }
    }
    out.push('\n');

    // Generate function stubs.
    for action in impl_actions {
        let fn_name = action
            .fn_name
            .as_deref()
            .unwrap_or(&action.name)
            .replace('-', "_");

        // Feature gate
        if let Some(feature) = &action.feature {
            out.push_str(&format!("#[cfg(feature = \"{}\")]\n", feature));
        }

        // Doc comment
        out.push_str(&format!(
            "/// `{}` — {} action (impl_required).\n",
            action.name, action.kind
        ));
        out.push_str("/// TODO: Implement this function.\n");

        // Determine if the function needs a generic <R: BloxRuntime>.
        // This is needed when any field type references R.
        let uses_r = action.fields.iter().any(|f| {
            let field_name = f.split(':').next().unwrap_or(f).trim();
            field_types
                .iter()
                .any(|ft| ft.name == field_name && ft.ty.contains('R'))
        }) || action.event_payload.as_ref().is_some_and(|p| {
            // If the payload type is generic (contains <...R...>), we need R.
            let payload_snake = p.replace('-', "_");
            let payload_type =
                resolve_payload_type(&payload_snake, message_variants, payload_types);
            payload_type.contains('<') && payload_type.contains('R')
        });

        // Signature
        let params = generate_fn_params(action, field_types, message_variants, payload_types);

        if uses_r {
            out.push_str(&format!(
                "pub fn {}<R: bloxide_core::capability::BloxRuntime>({}) {{\n",
                fn_name, params
            ));
        } else {
            out.push_str(&format!("pub fn {}({}) {{\n", fn_name, params));
        }

        out.push_str("    // TODO: Implement action logic\n");
        out.push_str("}\n\n");
    }

    out
}

/// Generate the parameter list for a function stub.
fn generate_fn_params(
    action: &ContextActionConfig,
    field_types: &[FieldInfo],
    message_variants: &[MessageVariantInfo],
    payload_types: &[PayloadTypeInfo],
) -> String {
    let mut params: Vec<String> = Vec::new();

    // Field parameters
    for field_spec in &action.fields {
        let parts: Vec<&str> = field_spec.split(':').collect();
        let field_name = parts[0].trim();
        let access_mode = parts.get(1).map(|s| s.trim()).unwrap_or("");

        let ty = field_types
            .iter()
            .find(|f| f.name == field_name)
            .map(|f| f.ty.clone())
            .unwrap_or_else(|| format!("/* unknown field: {} */ ()", field_name));

        let param_name = format!("_{}", field_name);
        let param = match access_mode {
            "mut" => format!("{}: &mut {}", param_name, ty),
            "ref" => format!("{}: &{}", param_name, ty),
            _ => format!("{}: {}", param_name, ty),
        };
        params.push(param);
    }

    // Event payload parameter
    if let Some(payload) = &action.event_payload {
        let payload_snake = payload.replace('-', "_");
        let payload_type = resolve_payload_type(&payload_snake, message_variants, payload_types);
        params.push(format!("_{}: &{}", payload_snake, payload_type));
    }

    params.join(", ")
}

/// Resolve the event payload type name from the snake_case name.
///
/// Resolution order:
/// 1. Check `payload_types` (from event mailboxes) — these include full
///    generic args (e.g. `SpawnedWorker<PeerCtrl<WorkerMsg, R>, R>`).
/// 2. Check `message_variants` (from `[[messages]]` variants) — these are
///    just the type name without generics.
/// 3. Fallback: convert snake_case to PascalCase.
fn resolve_payload_type(
    payload_snake: &str,
    message_variants: &[MessageVariantInfo],
    payload_types: &[PayloadTypeInfo],
) -> String {
    // 1. Try the payload type map first (has generics).
    if let Some(pt) = payload_types.iter().find(|p| p.snake == payload_snake) {
        return pt.full_type.clone();
    }

    // 2. Try message variants.
    if let Some(v) = message_variants.iter().find(|v| v.snake == payload_snake) {
        return v.type_name.clone();
    }

    // 3. Fallback: convert snake_case to PascalCase.
    to_camel_case(payload_snake)
}
