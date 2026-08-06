// Copyright 2025 Bloxide, all rights reserved
//! Generate, then run.

use clap_cargo::Features;

pub fn run(cargo: Features, example: Option<String>, args: Vec<String>) -> anyhow::Result<()> {
    crate::forward::generate_then_forward("run", cargo, example, args)
}
