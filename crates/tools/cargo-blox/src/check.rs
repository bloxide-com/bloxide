// Copyright 2025 Bloxide, all rights reserved
//! Generate, then check.

use clap_cargo::Features;

pub fn check(cargo: Features, args: Vec<String>) -> anyhow::Result<()> {
    crate::generate::generate(None)?;
    let mut extra = Vec::new();
    if !cargo.features.is_empty() {
        extra.push("--features".to_string());
        extra.push(cargo.features.join(","));
    }
    if cargo.no_default_features {
        extra.push("--no-default-features".to_string());
    }
    if cargo.all_features {
        extra.push("--all-features".to_string());
    }
    extra.extend(args);
    crate::forward::forward_to_cargo("check", &extra)
}
