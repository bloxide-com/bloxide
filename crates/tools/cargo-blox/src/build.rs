// Copyright 2025 Bloxide, all rights reserved
//! Generate, then build.

use clap_cargo::Features;

pub fn build(cargo: Features, example: Option<String>, args: Vec<String>) -> anyhow::Result<()> {
    crate::forward::generate_then_forward("build", cargo, example, args)
}
