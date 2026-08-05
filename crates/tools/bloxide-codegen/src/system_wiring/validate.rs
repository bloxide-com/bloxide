// Copyright 2025 Bloxide, all rights reserved
//! Validation of a `system.toml` wiring manifest against the discovered blox
//! configs, run before any main.rs emission.

use super::actor_kind::skip_in_main_body;
use super::ctor_fields::{collect_all_ctor_field_names, collect_ctor_fields};
use crate::schema::{BloxConfig, SystemConfig};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate(
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
