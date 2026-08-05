// Copyright 2025 Bloxide, all rights reserved
//! Constructor-field collection: derive the `Ctx::new(...)` parameter list of
//! a blox context, honoring per-field Cargo feature gates.

use crate::schema::BloxConfig;
use std::collections::{BTreeMap, BTreeSet};

/// A constructor field in declaration order (state fields excluded).
#[derive(Debug, Clone)]
pub(super) struct CtorField {
    pub(super) name: String,
    pub(super) is_self_id: bool,
}

/// Collect the constructor fields of a blox context in the order they appear
/// in the generated `Ctx::new(...)` signature.
///
/// `active_features` maps blox crate names (e.g. `"pool-blox"`) to the set of
/// Cargo features enabled on that dependency in the app's Cargo.toml. Fields
/// with a `feature` attribute that is not in the active set are silently
/// skipped — they don't exist in the compiled struct when the feature is off.
pub(super) fn collect_ctor_fields(
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
pub(super) fn collect_all_ctor_field_names(blox_config: &BloxConfig) -> BTreeSet<String> {
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
