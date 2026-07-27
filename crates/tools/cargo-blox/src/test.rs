// Copyright 2025 Bloxide, all rights reserved
//! Generate, then test.

use clap_cargo::Features;

pub fn test(cargo: Features, args: Vec<String>) -> anyhow::Result<()> {
    crate::forward::generate_then_forward("test", cargo, args)
}
