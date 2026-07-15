// Copyright 2025 Bloxide, all rights reserved
//! Generate, then test.

use clap_cargo::Features;

pub fn test(cargo: Features, args: Vec<String>) -> anyhow::Result<()> {
    crate::generate::generate(None)?;
    let mut extra = Vec::new();
    for feature in cargo.features {
        extra.push("--features".to_string());
        extra.push(feature);
    }
    if cargo.no_default_features {
        extra.push("--no-default-features".to_string());
    }
    if cargo.all_features {
        extra.push("--all-features".to_string());
    }
    extra.extend(args);
    crate::forward::forward_to_cargo("test", &extra)
}
