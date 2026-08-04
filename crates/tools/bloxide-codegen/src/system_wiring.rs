// Copyright 2025 Bloxide, all rights reserved
//! Generate a complete binary `main.rs` from a `system.toml` wiring manifest.

use crate::schema::{BloxConfig, SystemConfig};
use crate::util::HEADER;
use quote::{format_ident, quote};
use std::collections::{BTreeMap, BTreeSet};

fn crate_name(s: &str) -> String {
    s.replace("-", "_")
}

/// Returns true if this actor is a timer service actor (no blox.toml, no
/// channels, no task, no context — just a timer mailbox spawned directly).
fn is_timer(actor: &crate::schema::ActorInstance) -> bool {
    actor.kind.as_deref() == Some("timer")
}

/// Returns true if this actor is dynamically spawned (concrete spec generated
/// at system level, but no channels/task/bootstrap/context in main.rs — the
/// impl crate's spawn function handles construction at runtime).
fn is_dynamic(actor: &crate::schema::ActorInstance) -> bool {
    actor.kind.as_deref() == Some("dynamic")
}

/// Returns true if this actor should be skipped during main.rs body generation
/// (channels, tasks, context, machine, bootstrap). Both timer and dynamic
/// actors are skipped — they have no presence in the generated main function.
fn skip_in_main_body(actor: &crate::schema::ActorInstance) -> bool {
    is_timer(actor) || is_dynamic(actor)
}

/// A constructor field in declaration order (state fields excluded).
#[derive(Debug, Clone)]
struct CtorField {
    name: String,
    is_self_id: bool,
}

/// Collect the constructor fields of a blox context in the order they appear
/// in the generated `Ctx::new(...)` signature.
///
/// `active_features` maps blox crate names (e.g. `"pool-blox"`) to the set of
/// Cargo features enabled on that dependency in the app's Cargo.toml. Fields
/// with a `feature` attribute that is not in the active set are silently
/// skipped — they don't exist in the compiled struct when the feature is off.
fn collect_ctor_fields(
    blox_config: &BloxConfig,
    blox_crate_name: &str,
    active_features: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<CtorField> {
    let mut fields = Vec::new();

    // Resolve the set of enabled features for this blox crate.
    let enabled = active_features.get(blox_crate_name);

    if let Some(context) = &blox_config.context {
        // 0. self_id is always FIRST in the constructor (matches struct order).
        fields.push(CtorField {
            name: "self_id".to_string(),
            is_self_id: true,
        });

        // 1. context.uses entries that contribute a field.
        for u in &context.uses {
            // Skip uses whose feature is not enabled.
            if let Some(feat) = &u.feature {
                if !enabled.map(|s| s.contains(feat)).unwrap_or(false) {
                    continue;
                }
            }
            // Single-field entry: use `field` (singular).
            if let Some(field_name) = &u.field {
                if u.role.as_deref() == Some("state") {
                    continue;
                }
                fields.push(CtorField {
                    name: field_name.clone(),
                    is_self_id: false,
                });
            }
            // Multi-field entry: use `fields` (plural) — iterate sub-fields.
            for sub in &u.fields {
                if sub.role.as_deref() == Some("state") {
                    continue;
                }
                fields.push(CtorField {
                    name: sub.name.clone(),
                    is_self_id: false,
                });
            }
        }
    }

    fields
}

/// All constructor field names ignoring feature gates — used to distinguish
/// "inject targets a feature-gated-off field" (tolerated) from "inject
/// targets a field that does not exist at all" (hard error, usually a typo).
fn collect_all_ctor_field_names(blox_config: &BloxConfig) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    names.insert("self_id".to_string());
    if let Some(context) = &blox_config.context {
        for u in &context.uses {
            if let Some(field_name) = &u.field {
                if u.role.as_deref() != Some("state") {
                    names.insert(field_name.clone());
                }
            }
            for sub in &u.fields {
                if sub.role.as_deref() != Some("state") {
                    names.insert(sub.name.clone());
                }
            }
        }
    }
    names
}

fn toml_value_to_tokens(value: &toml::Value) -> anyhow::Result<proc_macro2::TokenStream> {
    use proc_macro2::Span;
    match value {
        toml::Value::Integer(i) => {
            let lit = proc_macro2::Literal::i64_unsuffixed(*i);
            Ok(quote! { #lit })
        }
        toml::Value::String(s) => {
            let lit = syn::LitStr::new(s, Span::call_site());
            Ok(quote! { #lit })
        }
        toml::Value::Float(f) => {
            let lit = proc_macro2::Literal::f64_unsuffixed(*f);
            Ok(quote! { #lit })
        }
        toml::Value::Boolean(true) => Ok(quote! { true }),
        toml::Value::Boolean(false) => Ok(quote! { false }),
        toml::Value::Array(arr) => {
            let elems: Vec<_> = arr
                .iter()
                .map(toml_value_to_tokens)
                .collect::<Result<_, _>>()?;
            Ok(quote! { [#(#elems),*] })
        }
        toml::Value::Datetime(dt) => {
            let lit = syn::LitStr::new(&dt.to_string(), Span::call_site());
            Ok(quote! { #lit })
        }
        toml::Value::Table(_) => {
            anyhow::bail!("nested tables are not supported as bootstrap payload values")
        }
    }
}

/// Replace the generic type parameter `R` with the concrete runtime name
/// in a message_path string.
///
/// Handles all positions where `R` can appear as a type argument:
///   - `<R>`           → `<TokioRuntime>`
///   - `<R: BloxRuntime>` → `<TokioRuntime>`
///   - `<Foo, R>`      → `<Foo, TokioRuntime>`
///   - `<R, Bar>`      → `<TokioRuntime, Bar>`
///   - `<Foo<R>>`     → `<Foo<TokioRuntime>>`
///
/// Scans the string char-by-char, replacing standalone `R` identifiers
/// that appear inside angle brackets (as generic arguments), avoiding
/// partial matches like `PeerCtrl`.
fn substitute_runtime_generic(path: &str, runtime_name: &str) -> String {
    // First handle the explicit `<R: BloxRuntime>` form.
    let s = path.replace("<R: BloxRuntime>", &format!("<{}>", runtime_name));

    // Scan and replace standalone `R` inside angle brackets.
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    let mut depth: i32 = 0;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        match c {
            '<' => {
                depth += 1;
                result.push(c);
            }
            '>' => {
                depth -= 1;
                result.push(c);
            }
            'R' if depth > 0 => {
                // Check this is a standalone identifier: not preceded or
                // followed by an identifier character.
                let prev_is_ident =
                    i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
                let next_is_ident =
                    i + 1 < chars.len() && (chars[i + 1].is_alphanumeric() || chars[i + 1] == '_');

                if !prev_is_ident && !next_is_ident {
                    // Standalone `R` inside generics — replace with runtime name.
                    result.push_str(runtime_name);
                } else {
                    result.push(c);
                }
            }
            _ => {
                result.push(c);
            }
        }
        i += 1;
    }

    result
}

/// Return the primary mailbox message type and its crate from a blox config.
fn primary_message(blox_config: &BloxConfig) -> Option<(String, String)> {
    let event = blox_config.event.as_ref()?;
    let mailbox = event.mailboxes.first()?;
    let message_path = mailbox.message_path.as_deref().unwrap_or(&mailbox.message);
    let parts: Vec<&str> = message_path.split("::").collect();
    if parts.len() >= 2 {
        // Strip generic args from the type name (e.g. "PeerCtrl<pool_messages"
        // becomes "PeerCtrl" when the path is "bloxide_peers::PeerCtrl<...>").
        let type_name = parts[1].split('<').next().unwrap_or(parts[1]);
        Some((parts[0].to_string(), type_name.to_string()))
    } else {
        let type_name = parts[0].split('<').next().unwrap_or(parts[0]);
        Some((type_name.to_string(), mailbox.message.clone()))
    }
}

/// Extract all crate names referenced in a message_path string.
///
/// A message_path like `pool_messages::SpawnedWorker<bloxide_peers::PeerCtrl<pool_messages::WorkerMsg, R>, R>`
/// references two crates: `pool_messages` and `bloxide_peers`.
///
/// Scans for `crate_name::` patterns, handling nested generics.
pub fn extract_crates_from_path(path: &str) -> Vec<String> {
    let mut crates = Vec::new();
    let bytes = path.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Accumulate an identifier.
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        if i > start {
            let ident = &path[start..i];
            // Check if followed by `::`
            if i + 1 < bytes.len() && bytes[i] == b':' && bytes[i + 1] == b':' {
                crates.push(ident.to_string());
            }
        } else {
            // Skip non-identifier char.
            i += 1;
        }
    }

    crates
}

/// Collect all crate names referenced by any mailbox's message_path in a blox config.
/// Returns a set of crate names (with underscores, not hyphensated).
fn all_message_crates(blox_config: &BloxConfig) -> BTreeSet<String> {
    let mut crates = BTreeSet::new();
    if let Some(event) = &blox_config.event {
        for mailbox in &event.mailboxes {
            let path = mailbox.message_path.as_deref().unwrap_or(&mailbox.message);
            for c in extract_crates_from_path(path) {
                crates.insert(c);
            }
        }
    }
    crates
}

fn validate(
    config: &SystemConfig,
    blox_configs: &BTreeMap<String, BloxConfig>,
    active_features: &BTreeMap<String, BTreeSet<String>>,
) -> anyhow::Result<()> {
    let actor_names: BTreeSet<String> = config.actors.iter().map(|a| a.name.clone()).collect();

    // The supervisor is an implicit actor — its control_ref and notify_ref are
    // available for injection via `source = "actor", actor = "supervisor"`.
    let has_supervisor = !config.supervision.is_empty();

    for actor in &config.actors {
        if skip_in_main_body(actor) {
            continue;
        }
        if !blox_configs.contains_key(&actor.blox) {
            anyhow::bail!(
                "actor '{}' references unknown blox '{}'",
                actor.name,
                actor.blox
            );
        }
    }

    for actor in &config.actors {
        for (field_name, source) in &actor.inject {
            if source.source == "actor" {
                let src = source.actor.as_deref().unwrap_or("");
                if src == "supervisor" && has_supervisor {
                    // OK — implicit supervisor actor.
                    continue;
                }
                if !actor_names.contains(src) {
                    anyhow::bail!(
                        "actor '{}' inject field '{}' references unknown actor '{}'",
                        actor.name,
                        field_name,
                        src
                    );
                }
            }
        }
    }

    for actor in &config.actors {
        if skip_in_main_body(actor) {
            continue;
        }
        let blox_config = blox_configs.get(&actor.blox).ok_or_else(|| {
            anyhow::anyhow!(
                "actor '{}' references unknown blox '{}'",
                actor.name,
                actor.blox
            )
        })?;
        let ctor_fields = collect_ctor_fields(blox_config, &actor.blox, active_features);
        let ctor_names: BTreeSet<String> = ctor_fields.iter().map(|f| f.name.clone()).collect();
        let all_ctor_names = collect_all_ctor_field_names(blox_config);

        for field_name in actor.inject.keys() {
            if !ctor_names.contains(field_name) {
                // Tolerated only when the field exists but its feature gate is
                // off — the injection is cfg'd out together with the field.
                // Anything else (typically a typo) is a hard error.
                if !all_ctor_names.contains(field_name) {
                    anyhow::bail!(
                        "actor '{}' injects unknown constructor field '{}' (blox '{}')",
                        actor.name,
                        field_name,
                        actor.blox
                    );
                }
            }
        }

        for field in &ctor_fields {
            if field.is_self_id {
                continue;
            }
            if !actor.inject.contains_key(&field.name) {
                anyhow::bail!(
                    "actor '{}' constructor field '{}' has no inject entry",
                    actor.name,
                    field.name
                );
            }
        }
    }

    Ok(())
}

pub fn generate(
    config: &SystemConfig,
    blox_configs: &BTreeMap<String, BloxConfig>,
    active_features: &BTreeMap<String, BTreeSet<String>>,
) -> anyhow::Result<String> {
    validate(config, blox_configs, active_features)?;

    let is_tokio = config.system.runtime == "tokio";
    let is_embassy = config.system.runtime == "embassy";
    if !is_tokio && !is_embassy {
        anyhow::bail!(
            "unsupported system runtime '{}', expected 'tokio' or 'embassy'",
            config.system.runtime
        );
    }

    let runtime_crate_ident = format_ident!(
        "{}",
        if is_tokio {
            "bloxide_tokio"
        } else {
            "bloxide_embassy"
        }
    );
    let runtime_ident = format_ident!(
        "{}",
        if is_tokio {
            "TokioRuntime"
        } else {
            "EmbassyRuntime"
        }
    );

    let runtime_ident_str = if is_tokio {
        "TokioRuntime".to_string()
    } else {
        "EmbassyRuntime".to_string()
    };

    let binary_name = config.system.name.as_deref().unwrap_or("main");
    let done_str = format!("{} complete", binary_name);
    let done_lit = syn::LitStr::new(&done_str, proc_macro2::Span::call_site());

    // ── Imports ─────────────────────────────────────────────────────────────
    let mut use_stmts = Vec::new();
    use_stmts.push(quote! {
        use ::#runtime_crate_ident::prelude::*;
    });
    use_stmts.push(quote! {
        use ::bloxide_core::lifecycle::LifecycleCommand;
    });

    // (No feature-gated imports needed — unified SupervisorSpec<R> has no
    // extra type parameters beyond R.)

    // Blox crate imports.
    let mut has_non_timer_actors = false;
    for actor in &config.actors {
        if actor.kind.as_deref() == Some("timer") {
            continue;
        }
        has_non_timer_actors = true;
        let blox_crate_ident = format_ident!("{}", crate_name(&actor.blox));
        use_stmts.push(quote! {
            use ::#blox_crate_ident::prelude::*;
        });
    }

    // Import concrete specs from the app's generated/ directory.
    // Explicit imports shadow the stub Spec types from the glob imports
    // above (Rust: explicit `use` takes precedence over glob `use *`).
    for actor in &config.actors {
        if actor.kind.as_deref() == Some("timer") {
            continue;
        }
        // Look up the actor name from the blox config to get the Spec name.
        let blox_config = blox_configs.get(&actor.blox);
        let blox_actor_name = blox_config
            .and_then(|bc| bc.actor.as_ref())
            .map(|a| a.name.clone())
            .unwrap_or_else(|| actor.name.clone());
        let spec_ident = format_ident!("{}Spec", blox_actor_name);
        // Module name is derived from the system.toml actor name, not the
        // blox.toml actor name (e.g. "worker" → "worker_spec_skeleton").
        let module_name = format_ident!(
            "{}_spec_skeleton",
            actor.name.replace('-', "_").to_lowercase()
        );
        use_stmts.push(quote! {
            use crate::generated::#module_name::#spec_ident;
        });
    }

    // Message type imports and bootstrap struct imports.
    let mut bootstrap_imports: BTreeSet<(String, String)> = BTreeSet::new();
    for actor in &config.actors {
        if skip_in_main_body(actor) {
            continue;
        }
        if let Some(blox_config) = blox_configs.get(&actor.blox) {
            if let Some((msg_crate, _msg_type)) = primary_message(blox_config) {
                for boot in &actor.bootstrap {
                    let variant = boot
                        .message
                        .split("::")
                        .nth(1)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "actor '{}' bootstrap message '{}' is not in the form MsgType::VariantName",
                                actor.name,
                                boot.message
                            )
                        })?;
                    bootstrap_imports.insert((msg_crate.clone(), variant.to_string()));
                }
            }
        }
    }

    let mut message_imports: BTreeSet<(String, String)> = BTreeSet::new();
    for actor in &config.actors {
        if actor.kind.as_deref() == Some("timer") {
            continue;
        }
        if let Some(blox_config) = blox_configs.get(&actor.blox) {
            if let Some((msg_crate, msg_type)) = primary_message(blox_config) {
                message_imports.insert((msg_crate, msg_type));
            }
        }
    }
    for (msg_crate, variant) in &bootstrap_imports {
        let crate_ident = format_ident!("{}", msg_crate);
        let variant_ident = format_ident!("{}", variant);
        use_stmts.push(quote! {
            use ::#crate_ident::#variant_ident;
        });
    }
    for (msg_crate, msg_type) in &message_imports {
        let crate_ident = format_ident!("{}", msg_crate);
        let type_ident = format_ident!("{}", msg_type);
        use_stmts.push(quote! {
            use ::#crate_ident::#type_ident;
        });
    }

    // Crate-level imports for all crates referenced in any mailbox message_path.
    // This handles multi-mailbox actors whose secondary mailbox types reference
    // crates beyond the primary message crate (e.g. bloxide_peers in
    // pool_messages::SpawnedWorker<bloxide_peers::PeerCtrl<...>, R>).
    // We import the crate root so that fully-qualified paths in the channels!
    // macro call resolve correctly.
    let mut all_crates: BTreeSet<String> = BTreeSet::new();
    for actor in &config.actors {
        if actor.kind.as_deref() == Some("timer") {
            continue;
        }
        if let Some(blox_config) = blox_configs.get(&actor.blox) {
            for c in all_message_crates(blox_config) {
                all_crates.insert(c);
            }
        }
    }
    // Also add crates from bootstrap and message type imports.
    for (msg_crate, _) in &bootstrap_imports {
        all_crates.insert(msg_crate.clone());
    }
    for (msg_crate, _) in &message_imports {
        all_crates.insert(msg_crate.clone());
    }
    for crate_name in all_crates {
        let crate_ident = format_ident!("{}", crate_name);
        use_stmts.push(quote! {
            use ::#crate_ident;
        });
    }

    // ── Timer spawn ─────────────────────────────────────────────────────────
    let mut embassy_timer_task_decl = Vec::new();
    let mut timer_stmts = Vec::new();

    let has_timer = config
        .actors
        .iter()
        .any(|a| a.kind.as_deref() == Some("timer"));
    if has_timer {
        if is_tokio {
            timer_stmts.push(quote! {
                let timer_ref = ::#runtime_crate_ident::spawn_timer!(8);
            });
        } else {
            embassy_timer_task_decl.push(quote! {
                ::#runtime_crate_ident::timer_task!(timer_task);
            });
            timer_stmts.push(quote! {
                let timer_ref = ::#runtime_crate_ident::spawn_timer!(spawner, timer_task, 8);
            });
        }
    }

    // ── Channel creation ────────────────────────────────────────────────────
    let mut channel_stmts = Vec::new();
    for actor in &config.actors {
        if skip_in_main_body(actor) {
            continue;
        }
        let id_ident = format_ident!("{}_id", actor.name);
        let mbox_ident = format_ident!("{}_mbox", actor.name);

        let blox_config = blox_configs.get(&actor.blox).ok_or_else(|| {
            anyhow::anyhow!(
                "actor '{}' references unknown blox '{}'",
                actor.name,
                actor.blox
            )
        })?;
        let event = blox_config.event.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "actor '{}' blox '{}' has no event config",
                actor.name,
                actor.blox
            )
        })?;

        // Collect mailboxes, respecting feature gates.
        // A mailbox is included when:
        //   - it has no feature gate, OR
        //   - its feature is enabled in the app's Cargo.toml for this blox.
        let enabled = active_features.get(&actor.blox);
        let mailboxes: Vec<_> = event
            .mailboxes
            .iter()
            .filter(|m| match &m.feature {
                None => true,
                Some(feat) => enabled.map(|s| s.contains(feat)).unwrap_or(false),
            })
            .collect();

        if mailboxes.is_empty() {
            anyhow::bail!("actor '{}' has no mailboxes", actor.name);
        }

        let capacity = actor.channel_capacity.unwrap_or(16);
        let capacity_lit = proc_macro2::Literal::usize_unsuffixed(capacity);

        // Build ref names: primary is {actor}_ref, secondaries are looked up
        // from inject entries with source = "self_secondary".
        let primary_ref_ident = format_ident!("{}_ref", actor.name);
        let mut ref_idents = vec![primary_ref_ident.clone()];
        let mut msg_type_tokens = Vec::new();

        // Primary mailbox message type.
        let primary = &mailboxes[0];
        let primary_path = primary.message_path.as_deref().unwrap_or(&primary.message);
        let primary_msg = substitute_runtime_generic(primary_path, &runtime_ident_str);
        let primary_msg_tokens: proc_macro2::TokenStream =
            syn::parse_str(&primary_msg).map_err(|e| {
                anyhow::anyhow!(
                    "failed to parse message type '{}' for actor '{}': {}",
                    primary_msg,
                    actor.name,
                    e
                )
            })?;
        msg_type_tokens.push(quote! { #primary_msg_tokens(#capacity_lit) });

        // Secondary mailboxes.
        for (i, mbox) in mailboxes.iter().enumerate().skip(1) {
            // Find the inject field that references this secondary mailbox.
            let secondary_field = actor
                .inject
                .iter()
                .find(|(_, src)| {
                    src.source == "self_secondary"
                        && src.index.unwrap_or(1) == i
                })
                .map(|(name, _)| name.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "actor '{}' has mailbox {} but no inject entry with source = 'self_secondary' and index = {}",
                        actor.name,
                        i,
                        i
                    )
                })?;
            let ref_ident = format_ident!("{}", secondary_field);
            ref_idents.push(ref_ident);

            let path = mbox.message_path.as_deref().unwrap_or(&mbox.message);
            let msg = substitute_runtime_generic(path, &runtime_ident_str);
            let msg_tokens: proc_macro2::TokenStream = syn::parse_str(&msg).map_err(|e| {
                anyhow::anyhow!(
                    "failed to parse secondary message type '{}' for actor '{}': {}",
                    msg,
                    actor.name,
                    e
                )
            })?;
            let cap_lit = proc_macro2::Literal::usize_unsuffixed(capacity);
            msg_type_tokens.push(quote! { #msg_tokens(#cap_lit) });
        }

        // Generate the channels! call with all message types.
        // Single mailbox: let ((ref,), mbox) = channels! { Msg(cap), };
        // Multi mailbox:  let ((ref1, ref2), mbox) = channels! { Msg1(cap), Msg2(cap), };
        // The trailing comma after #(#ref_idents),* is critical for the
        // single-mailbox case: `((ref,), mbox)` matches `(ActorRef,)` while
        // `((ref), mbox)` would bind `ref` to the tuple `(ActorRef,)`.
        channel_stmts.push(quote! {
            let ((#(#ref_idents,)*), #mbox_ident) = ::#runtime_crate_ident::channels! {
                #(#msg_type_tokens),*,
            };
            let #id_ident = #primary_ref_ident.id();
        });
    }

    // (Spawn channel creation removed — the dynamic spawn factory mechanism
    // was removed in spawn-architecture-v2. Dynamic spawning is now handled
    // via factory injection in the blox's context, wired through the
    // supervisor's RegisterDynamicChild control message.)

    // (Factory construction removed — see spawn-architecture-v2.)

    // ── Supervisor phase 1: create builders, extract refs ──────────────────
    //
    // The supervisor's control_ref and notify_ref must be available BEFORE
    // context construction so actors (e.g. the Pool) can inject them.
    // Children are added in phase 2 (after machine construction).
    //
    // Symbol table: maps (actor_name, field_name) → variable ident.
    // - (supervisor, "primary") → {supervisor}_ref (not used, but registered)
    // - (supervisor, "control") → extracted control_ref from ChildGroupBuilder
    // - (supervisor, "notify")  → extracted notify_ref from ChildGroupBuilder
    let mut symbol_table: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut supervisor_setup_stmts = Vec::new();
    let mut supervisor_finish_stmts = Vec::new();
    let mut root_task_decls = Vec::new();

    for (idx, sup) in config.supervision.iter().enumerate() {
        let group_ident = if config.supervision.len() == 1 {
            format_ident!("group")
        } else {
            format_ident!("group_{}", idx)
        };
        let sup_ctx_ident = if config.supervision.len() == 1 {
            format_ident!("sup_ctx")
        } else {
            format_ident!("sup_ctx_{}", idx)
        };
        let sup_machine_ident = if config.supervision.len() == 1 {
            format_ident!("sup_machine")
        } else {
            format_ident!("sup_machine_{}", idx)
        };
        let sup_notify_rx_ident = if config.supervision.len() == 1 {
            format_ident!("sup_notify_rx")
        } else {
            format_ident!("sup_notify_rx_{}", idx)
        };
        let sup_control_rx_ident = if config.supervision.len() == 1 {
            format_ident!("sup_control_rx")
        } else {
            format_ident!("sup_control_rx_{}", idx)
        };
        let sup_id_ident = if config.supervision.len() == 1 {
            format_ident!("sup_id")
        } else {
            format_ident!("sup_id_{}", idx)
        };
        let task_ident = if config.supervision.len() == 1 {
            format_ident!("supervisor_task")
        } else {
            format_ident!("supervisor_task_{}", idx)
        };
        let control_ref_ident = format_ident!("sup_control_ref_{}", idx);
        let notify_ref_ident = format_ident!("sup_notify_ref_{}", idx);

        // Map strategy to GroupShutdown. Only the two strategies matching
        // GroupShutdown's semantics exist; anything else is a hard error
        // (previously every unknown value silently became WhenAnyDone).
        let shutdown_strategy = match sup.strategy.as_str() {
            "when_all_done" => quote! { GroupShutdown::WhenAllDone },
            "when_any_done" => quote! { GroupShutdown::WhenAnyDone },
            other => anyhow::bail!(
                "unknown supervision strategy '{other}' — expected `when_any_done` or `when_all_done`"
            ),
        };

        // Phase 1: create builder + extract control_ref and notify_ref.
        // Capacities: explicit const generics (expression position does not
        // apply const-parameter defaults). Defaults: notify sized to the
        // child count (every child can have a terminal report in flight),
        // control 16, per-child lifecycle 4. These channels carry the
        // supervision control plane — sized to make "full" a bug, not
        // routine backpressure (confirm-before-record still recovers).
        let notify_cap = sup
            .capacities
            .notify
            .unwrap_or_else(|| (2 * sup.children.len()).max(32));
        let control_cap = sup.capacities.control.unwrap_or(16);
        let lifecycle_cap = sup.capacities.lifecycle.unwrap_or(4);
        for (name, cap) in [
            ("notify", notify_cap),
            ("control", control_cap),
            ("lifecycle", lifecycle_cap),
        ] {
            if cap == 0 {
                anyhow::bail!("supervision capacities.{name} must be >= 1");
            }
        }
        supervisor_setup_stmts.push(quote! {
            let mut #group_ident = ChildGroupBuilder::<_, _, #notify_cap, #control_cap, #lifecycle_cap>::new(#shutdown_strategy);
            let #control_ref_ident = #group_ident.control_ref();
            let #notify_ref_ident = #group_ident.notify_ref();
        });

        // Register refs in symbol table.
        // The supervisor actor name comes from the supervision entry —
        // we use "supervisor" as the canonical name (matching the system.toml
        // `actor = "supervisor"` convention).
        let sup_name = "supervisor".to_string();
        symbol_table.insert(
            (sup_name.clone(), "control".to_string()),
            control_ref_ident.to_string(),
        );
        symbol_table.insert(
            (sup_name.clone(), "notify".to_string()),
            notify_ref_ident.to_string(),
        );

        // Phase 2: add children, finish, construct supervisor (after machines).
        for child_name in &sup.children {
            let child_actor = config
                .actors
                .iter()
                .find(|a| &a.name == child_name)
                .ok_or_else(|| {
                    anyhow::anyhow!("supervision child '{}' not declared", child_name)
                })?;
            let child_mbox_ident = format_ident!("{}_mbox", child_actor.name);
            let child_id_ident = format_ident!("{}_id", child_actor.name);
            let child_machine_ident = format_ident!("{}_machine", child_actor.name);
            let child_task_ident = format_ident!("{}_task", child_actor.name);

            let policy = if let Some(policy_config) = sup.policies.get(child_name) {
                if let Some(restart) = &policy_config.restart {
                    if restart.max == 0 {
                        anyhow::bail!(
                            "supervision policy for '{child_name}': restart.max must be >= 1 \
                             (max = 0 forbids any restart — use `stop = true` instead)"
                        );
                    }
                    let max = restart.max;
                    quote! { ChildPolicy::Reset { max: #max } }
                } else {
                    quote! { ChildPolicy::Stop }
                }
            } else {
                quote! { ChildPolicy::Stop }
            };

            if is_tokio {
                supervisor_finish_stmts.push(quote! {
                    ::#runtime_crate_ident::spawn_child!(
                        #group_ident,
                        #child_task_ident(#child_machine_ident, #child_mbox_ident, #child_id_ident),
                        #policy
                    );
                });
            } else {
                supervisor_finish_stmts.push(quote! {
                    ::#runtime_crate_ident::spawn_child!(
                        spawner,
                        #group_ident,
                        #child_task_ident(#child_machine_ident, #child_mbox_ident, #child_id_ident),
                        #policy
                    );
                });
            }
        }

        supervisor_finish_stmts.push(quote! {
            let #sup_id_ident = ::#runtime_crate_ident::next_actor_id!();
            let (children, #sup_notify_rx_ident, #sup_control_rx_ident) = #group_ident.finish();
        });

        // Use the system-level generated concrete supervisor spec, not the
        // blox-crate-level stub. The concrete spec has real action closures
        // wired from the context crates.
        let supervisor_spec_path: syn::Path =
            syn::parse_str("crate::generated::bloxide_supervisor_spec_skeleton::SupervisorSpec")
                .expect("valid supervisor spec path");
        let supervisor_ctx_path: syn::Path = syn::parse_str("::bloxide_supervisor::SupervisorCtx")
            .expect("valid supervisor ctx path");
        let supervisor_event_path: syn::Path =
            syn::parse_str("::bloxide_supervisor::SupervisorEvent")
                .expect("valid supervisor event path");

        supervisor_finish_stmts.push(quote! {
            let #sup_ctx_ident = #supervisor_ctx_path::new(#sup_id_ident, children, #notify_ref_ident);
            let mut #sup_machine_ident = ::bloxide_core::StateMachine::<#supervisor_spec_path<#runtime_ident>>::new(#sup_ctx_ident);
            #sup_machine_ident.dispatch(#supervisor_event_path::<#runtime_ident>::Lifecycle(LifecycleCommand::Start));
        });

        if is_tokio {
            root_task_decls.push(quote! {
                ::#runtime_crate_ident::root_task!(#task_ident, #supervisor_spec_path<#runtime_ident>);
            });
        } else {
            // Embassy: use the two-argument form (no std::process::exit).
            // The root task simply returns when the supervisor stops.
            // On std targets (arch-std), the executor stays alive idle —
            // the process can be terminated with Ctrl-C or a hardware reset.
            // On no_std targets, std::process::exit doesn't exist.
            root_task_decls.push(quote! {
                ::#runtime_crate_ident::root_task!(#task_ident, #supervisor_spec_path<#runtime_ident>);
            });
        }
    }

    // ── Actor task declarations (file level) ──────────────────────────────
    let mut task_decls = Vec::new();
    for actor in &config.actors {
        if skip_in_main_body(actor) {
            continue;
        }
        let blox_config = blox_configs.get(&actor.blox).ok_or_else(|| {
            anyhow::anyhow!(
                "actor '{}' references unknown blox '{}'",
                actor.name,
                actor.blox
            )
        })?;
        let actor_name = blox_config
            .actor
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| actor.name.clone());
        let spec_ident = format_ident!("{}Spec", actor_name);
        let task_ident = format_ident!("{}_task", actor.name);

        let generics_str = blox_config
            .context
            .as_ref()
            .and_then(|c| c.generics.as_deref())
            .unwrap_or("");

        let has_r = generics_str.contains("R:");

        let spec_ty = if has_r {
            quote! { #spec_ident<#runtime_ident> }
        } else {
            quote! { #spec_ident }
        };

        task_decls.push(quote! {
            ::#runtime_crate_ident::actor_task_supervised!(#task_ident, #spec_ty);
        });
    }

    // ── Dynamic actor spawn wrappers ────────────────────────────────────────
    // For each dynamic actor, build a map from impl_crate → (spec_module, spec_type)
    // so that factory injection can generate monomorphization wrappers.
    let mut dynamic_actor_specs: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for actor in &config.actors {
        if is_dynamic(actor) {
            if let Some(impl_crate) = &actor.impl_crate {
                let blox_config = blox_configs.get(&actor.blox).ok_or_else(|| {
                    anyhow::anyhow!(
                        "dynamic actor '{}' references unknown blox '{}'",
                        actor.name,
                        actor.blox
                    )
                })?;
                let blox_actor_name = blox_config
                    .actor
                    .as_ref()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| actor.name.clone());
                let spec_ident_str = format!("{}Spec", blox_actor_name);
                let spec_module = format!(
                    "{}_spec_skeleton",
                    actor.name.replace('-', "_").to_lowercase()
                );
                let generics_str = blox_config
                    .context
                    .as_ref()
                    .and_then(|c| c.generics.as_deref())
                    .unwrap_or("");
                let has_r = generics_str.contains("R:");
                let spec_ty = if has_r {
                    format!("{}<{}>", spec_ident_str, runtime_ident_str)
                } else {
                    spec_ident_str.clone()
                };
                dynamic_actor_specs.insert(impl_crate.clone(), (spec_module, spec_ty));
            }
        }
    }

    // ── Context construction ────────────────────────────────────────────────
    let mut ctx_stmts = Vec::new();
    for actor in &config.actors {
        if skip_in_main_body(actor) {
            continue;
        }
        let blox_config = blox_configs.get(&actor.blox).ok_or_else(|| {
            anyhow::anyhow!(
                "actor '{}' references unknown blox '{}'",
                actor.name,
                actor.blox
            )
        })?;
        let actor_name = blox_config
            .actor
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| actor.name.clone());
        let ctx_ident = format_ident!("{}Ctx", actor_name);
        let ctx_var_ident = format_ident!("{}_ctx", actor.name);
        let id_ident = format_ident!("{}_id", actor.name);

        let ctor_fields = collect_ctor_fields(blox_config, &actor.blox, active_features);
        let mut ctor_args = Vec::new();
        for field in &ctor_fields {
            if field.is_self_id {
                ctor_args.push(quote! { #id_ident });
            } else if let Some(source) = actor.inject.get(&field.name) {
                if source.source == "self" {
                    let ref_ident = format_ident!("{}_ref", actor.name);
                    ctor_args.push(quote! { #ref_ident.clone() });
                } else if source.source == "self_secondary" {
                    // The ref was already created by the multi-mailbox channels! call
                    // and named after this inject field name.
                    let ref_ident = format_ident!("{}", field.name);
                    ctor_args.push(quote! { #ref_ident.clone() });
                } else if source.source == "factory" {
                    let factory_crate = source.crate_name.as_deref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "actor '{}' inject field '{}' source = 'factory' missing crate name",
                            actor.name,
                            field.name
                        )
                    })?;
                    let factory_fn = source.function.as_deref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "actor '{}' inject field '{}' source = 'factory' missing function name",
                            actor.name,
                            field.name
                        )
                    })?;
                    let crate_ident = format_ident!("{}", factory_crate);
                    let fn_ident = format_ident!("{}", factory_fn);

                    // If this factory crate is a dynamic actor's impl_crate,
                    // generate a monomorphization wrapper that fills in the
                    // system-level concrete spec type.
                    if let Some((spec_module, spec_ty)) = dynamic_actor_specs.get(factory_crate) {
                        let spec_module_ident = format_ident!("{}", spec_module);
                        let spec_ty_tokens: proc_macro2::TokenStream =
                            spec_ty.parse().map_err(|e| {
                                anyhow::anyhow!("failed to parse spec type '{}': {}", spec_ty, e)
                            })?;
                        let spec_path =
                            quote! { crate::generated::#spec_module_ident::#spec_ty_tokens };
                        // Use a closure that monomorphizes the generic spawn
                        // function with the system-level concrete spec type.
                        // The `as _` cast on the constructor argument tells
                        // Rust to infer the closure's parameter types from
                        // the target field type (a SpawnFn<R, Req>).
                        ctor_args.push(quote! {
                            (|req, notify| ::#crate_ident::#fn_ident::<#spec_path>(req, notify)) as _
                        });
                    } else {
                        ctor_args.push(quote! { ::#crate_ident::#fn_ident as _ });
                    }
                } else if source.source == "actor" {
                    let src_actor = source.actor.as_deref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "actor '{}' inject field '{}' source = 'actor' missing actor name",
                            actor.name,
                            field.name
                        )
                    })?;
                    let field_selector = source.field.as_deref().unwrap_or("primary");
                    if field_selector == "primary" {
                        // Default: inject the actor's primary channel ref.
                        let ref_ident = format_ident!("{}_ref", src_actor);
                        ctor_args.push(quote! { #ref_ident.clone() });
                    } else {
                        // Named ref: look up in symbol table (e.g. supervisor's
                        // "control" or "notify" refs).
                        let sym = symbol_table
                            .get(&(src_actor.to_string(), field_selector.to_string()))
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "actor '{}' has no ref '{}'",
                                    src_actor,
                                    field_selector
                                )
                            })?;
                        let ref_ident = format_ident!("{}", sym);
                        ctor_args.push(quote! { #ref_ident.clone() });
                    }
                } else {
                    anyhow::bail!(
                        "actor '{}' inject field '{}' has unsupported source '{}'",
                        actor.name,
                        field.name,
                        source.source
                    );
                }
            }
        }

        ctx_stmts.push(quote! {
            let #ctx_var_ident = #ctx_ident::new(#(#ctor_args),*);
        });
    }

    // ── Machine construction ────────────────────────────────────────────────
    let mut machine_stmts = Vec::new();
    for actor in &config.actors {
        if skip_in_main_body(actor) {
            continue;
        }
        let machine_ident = format_ident!("{}_machine", actor.name);
        let ctx_var_ident = format_ident!("{}_ctx", actor.name);
        machine_stmts.push(quote! {
            let #machine_ident = ::bloxide_core::StateMachine::new(#ctx_var_ident);
        });
    }

    // ── Supervisor run statements ───────────────────────────────────────────
    let mut supervisor_run_stmts = Vec::new();
    for (idx, _sup) in config.supervision.iter().enumerate() {
        let task_ident = if config.supervision.len() == 1 {
            format_ident!("supervisor_task")
        } else {
            format_ident!("supervisor_task_{}", idx)
        };
        let sup_machine_ident = if config.supervision.len() == 1 {
            format_ident!("sup_machine")
        } else {
            format_ident!("sup_machine_{}", idx)
        };
        let sup_notify_rx_ident = if config.supervision.len() == 1 {
            format_ident!("sup_notify_rx")
        } else {
            format_ident!("sup_notify_rx_{}", idx)
        };
        let sup_control_rx_ident = if config.supervision.len() == 1 {
            format_ident!("sup_control_rx")
        } else {
            format_ident!("sup_control_rx_{}", idx)
        };

        // Unified supervisor run — tuple mailboxes (child_rx, control_rx).
        // No feature gating, no SupervisorMailboxes, no spawn_rx.
        if is_tokio {
            supervisor_run_stmts.push(quote! {
                #task_ident(#sup_machine_ident, (#sup_notify_rx_ident, #sup_control_rx_ident)).await;
            });
        } else {
            supervisor_run_stmts.push(quote! {
                spawner.must_spawn(#task_ident(#sup_machine_ident, (#sup_notify_rx_ident, #sup_control_rx_ident)));
            });
        }
    }

    // ── Bootstrap message sends ─────────────────────────────────────────────
    let mut bootstrap_send_stmts = Vec::new();
    for actor in &config.actors {
        if skip_in_main_body(actor) {
            continue;
        }
        if actor.bootstrap.is_empty() {
            continue;
        }
        let ref_ident = format_ident!("{}_ref", actor.name);
        let id_ident = format_ident!("{}_id", actor.name);
        for boot in &actor.bootstrap {
            let parts: Vec<&str> = boot.message.split("::").collect();
            if parts.len() != 2 {
                anyhow::bail!(
                    "actor '{}' bootstrap message '{}' is not in the form MsgType::VariantName",
                    actor.name,
                    boot.message
                );
            }
            let msg_type_ident = format_ident!("{}", parts[0]);
            let variant_ident = format_ident!("{}", parts[1]);

            let msg_expr = if let Some(payload) = &boot.payload {
                let mut field_tokens = Vec::new();
                for (key, value) in payload {
                    let field_ident = format_ident!("{}", key);
                    let value_tokens = toml_value_to_tokens(value)?;
                    field_tokens.push(quote! { #field_ident: #value_tokens });
                }
                quote! { #msg_type_ident::#variant_ident(#variant_ident { #(#field_tokens),* }) }
            } else {
                quote! { #msg_type_ident::#variant_ident(#variant_ident) }
            };

            if is_tokio {
                bootstrap_send_stmts.push(quote! {
                    let _ = #ref_ident.send(#id_ident, #msg_expr).await;
                });
            } else {
                bootstrap_send_stmts.push(quote! {
                    let _ = #ref_ident.try_send(#id_ident, #msg_expr);
                });
            }
        }
    }

    // ── Main function ───────────────────────────────────────────────────────
    let main_fn = if is_tokio {
        quote! {
            async fn main() {
                tracing_log::LogTracer::init().ok();
                tracing_subscriber::fmt()
                    .with_env_filter(
                        tracing_subscriber::EnvFilter::try_from_default_env()
                            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                    )
                    .try_init()
                    .ok();

                #(#timer_stmts)*
                #(#channel_stmts)*
                #(#supervisor_setup_stmts)*
                #(#ctx_stmts)*
                #(#machine_stmts)*
                #(#supervisor_finish_stmts)*
                #(#bootstrap_send_stmts)*
                #(#supervisor_run_stmts)*
                println!(#done_lit);
            }
        }
    } else {
        quote! {
            fn main() {
                static EXECUTOR: ::static_cell::StaticCell<::embassy_executor::Executor> =
                    ::static_cell::StaticCell::new();
                let executor = EXECUTOR.init(::embassy_executor::Executor::new());
                executor.run(setup);
            }

            fn setup(spawner: ::embassy_executor::Spawner) {
                #(#timer_stmts)*
                #(#channel_stmts)*
                #(#supervisor_setup_stmts)*
                #(#ctx_stmts)*
                #(#machine_stmts)*
                #(#supervisor_finish_stmts)*
                #(#bootstrap_send_stmts)*
                #(#supervisor_run_stmts)*
                println!(#done_lit);
            }
        }
    };

    let tokens = quote! {
        #![allow(unused_imports, unused_variables)]
        #(#embassy_timer_task_decl)*
        #(#use_stmts)*
        #(#task_decls)*
        #(#root_task_decls)*
        #main_fn
    };

    // Prepend `mod generated;` if we generated concrete spec files.
    // This must come before any use statements that reference crate::generated.
    let tokens = if has_non_timer_actors {
        quote! {
            #![allow(unused_imports, unused_variables)]
            mod generated;
            #(#embassy_timer_task_decl)*
            #(#use_stmts)*
            #(#task_decls)*
            #(#root_task_decls)*
            #main_fn
        }
    } else {
        tokens
    };

    let file = syn::parse2::<syn::File>(tokens)
        .map_err(|e| anyhow::anyhow!("syn parsing failed: {}", e))?;
    let pretty = prettyplease::unparse(&file);

    // Apply the `#[tokio::main]` attribute that prettyplease can't emit
    // (it's not a regular attribute — it's an outer attribute on a function
    // that prettyplease formats on the same line).
    let with_attr = if is_tokio {
        pretty.replace("async fn main()", "#[tokio::main]\nasync fn main()")
    } else {
        pretty
    };

    // Run rustfmt on the output so it matches `cargo fmt` exactly.
    // This prevents `cargo blox generate` (which calls `cargo fmt`) from
    // clobbering the wire-generated main.rs with formatting diffs.
    let with_header = format!("{}{}", HEADER, with_attr);
    let formatted = rustfmt_source(&with_header)?;

    Ok(formatted)
}

/// Format a Rust source string through `rustfmt`.
///
/// Falls back to the unformatted input if `rustfmt` is not available,
/// so the codegen still works in environments without rustfmt installed.
pub(crate) fn rustfmt_source(source: &str) -> anyhow::Result<String> {
    use std::io::Write;
    use std::process::Command;

    let spawn_result = Command::new("rustfmt")
        .args(["--edition", "2021", "--emit", "stdout"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match spawn_result {
        Ok(c) => c,
        Err(_) => {
            // rustfmt not available — return the source as-is.
            return Ok(source.to_string());
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(source.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if output.status.success() {
        let formatted = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(formatted)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "bloxide: warning: rustfmt failed, returning unformatted code:\n{}",
            stderr
        );
        Ok(source.to_string())
    }
}
