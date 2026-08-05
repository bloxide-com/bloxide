// Copyright 2025 Bloxide, all rights reserved
//! Message-path helpers: runtime-generic substitution and crate-name scanning
//! for `message_path` strings from blox configs.

use crate::schema::BloxConfig;
use std::collections::BTreeSet;

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
pub(super) fn substitute_runtime_generic(path: &str, runtime_name: &str) -> String {
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
pub(super) fn primary_message(blox_config: &BloxConfig) -> Option<(String, String)> {
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
pub(super) fn all_message_crates(blox_config: &BloxConfig) -> BTreeSet<String> {
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
