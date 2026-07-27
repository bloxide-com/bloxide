// Copyright 2025 Bloxide, all rights reserved
//! Generate, then check.

use clap_cargo::Features;

pub fn check(cargo: Features, args: Vec<String>) -> anyhow::Result<()> {
    crate::forward::generate_then_forward("check", cargo, args)
}
