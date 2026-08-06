// Copyright 2025 Bloxide, all rights reserved
//! Forward commands to `cargo`.

use clap_cargo::Features;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn forward_to_cargo(cmd: &str, extra_args: &[String]) -> anyhow::Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let mut command = Command::new(&cargo);
    command.arg(cmd);

    for arg in extra_args {
        command.arg(arg);
    }

    let status = command.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

/// Generate code from blox.toml, then forward to a cargo subcommand.
///
/// Shared implementation for `build`, `check`, `test`, and `run` — they all
/// regenerate first, then forward to cargo with the same feature-flag
/// handling.
///
/// Without `--example`, `build` / `check` / `test` run in BOTH workspaces:
/// the repo workspace (`cargo <cmd> --workspace`, from the current dir —
/// feature flags and extra args apply here only) and the generated workspace
/// (`cargo <cmd> --workspace --manifest-path <root>/target/bloxide-generated/Cargo.toml`,
/// no extra args). `test` additionally runs each impl crate's tests via
/// `--manifest-path` — impl crates belong to neither workspace (see
/// `bloxide_codegen::blox_crate::impl_crate_dirs`).
///
/// With `--example <name>`, the command is scoped to that example crate in
/// the generated workspace (`cargo <cmd> --manifest-path <generated> -p <name>`).
/// `run` REQUIRES `--example` — examples are no longer root workspace
/// members; trailing args are passed to the binary after `--`.
pub fn generate_then_forward(
    cmd: &str,
    cargo: Features,
    example: Option<String>,
    args: Vec<String>,
) -> anyhow::Result<()> {
    crate::generate::generate(None)?;

    let root = crate::utils::find_workspace_root()?;
    let generated_manifest = root
        .join(bloxide_codegen::blox_crate::GENERATED_WORKSPACE_DIR)
        .join("Cargo.toml");

    if let Some(name) = example {
        return forward_to_example(cmd, cargo, &name, args, &root, &generated_manifest);
    }

    if cmd == "run" {
        anyhow::bail!(
            "`cargo blox run` requires `--example <name>`: examples are no longer root\n\
             workspace members — they are materialized into \
             target/bloxide-generated/examples/.\n\
             Available examples: {}",
            crate::utils::available_examples(&root).join(", ")
        );
    }

    let mut extra = Vec::new();
    extra.push("--workspace".to_string());
    push_feature_flags(&cargo, &mut extra);
    extra.extend(args);
    forward_to_cargo(cmd, &extra)?;

    // Generated workspace (pure-TOML blox crates + materialized examples).
    // Skipped when nothing has been materialized yet.
    if generated_manifest.exists() {
        println!("bloxide: cargo {} --workspace (generated workspace)", cmd);
        forward_to_cargo(
            cmd,
            &[
                "--workspace".to_string(),
                "--manifest-path".to_string(),
                generated_manifest.display().to_string(),
            ],
        )?;
    }

    // Impl crates (e.g. crates/impl/tokio-pool-demo-impl) belong to neither
    // workspace — root-excluded so a fresh clone loads, and cargo forbids
    // out-of-tree membership in the generated workspace (see
    // bloxide_codegen::blox_crate::impl_crate_dirs). They build as path
    // dependencies of the generated example crates; run their tests
    // explicitly against their own manifests.
    if cmd == "test" {
        for dir in bloxide_codegen::blox_crate::impl_crate_dirs(&root) {
            let manifest = root.join(&dir).join("Cargo.toml");
            if manifest.exists() {
                println!(
                    "bloxide: cargo test --manifest-path {} (impl crate)",
                    manifest.display()
                );
                forward_to_cargo(
                    "test",
                    &[
                        "--manifest-path".to_string(),
                        manifest.display().to_string(),
                    ],
                )?;
            }
        }
    }
    Ok(())
}

/// Forward a command scoped to one example crate in the generated workspace:
/// `cargo <cmd> --manifest-path <generated> -p <name> [--features ...] [-- <args>]`.
fn forward_to_example(
    cmd: &str,
    cargo: Features,
    name: &str,
    args: Vec<String>,
    root: &Path,
    generated_manifest: &PathBuf,
) -> anyhow::Result<()> {
    let example_dir = generated_manifest
        .parent()
        .unwrap()
        .join("examples")
        .join(name);
    if !example_dir.is_dir() {
        anyhow::bail!(crate::exit::not_found(format!(
            "example '{}' not found (expected {}).\nAvailable examples: {}",
            name,
            example_dir.display(),
            crate::utils::available_examples(root).join(", ")
        )));
    }

    let mut extra = vec![
        "--manifest-path".to_string(),
        generated_manifest.display().to_string(),
        "-p".to_string(),
        name.to_string(),
    ];
    push_feature_flags(&cargo, &mut extra);
    if cmd == "run" && !args.is_empty() {
        extra.push("--".to_string());
    }
    extra.extend(args);

    println!("bloxide: cargo {} -p {} (generated examples)", cmd, name);
    forward_to_cargo(cmd, &extra)
}

/// clap-cargo feature flags → cargo CLI args.
fn push_feature_flags(cargo: &Features, extra: &mut Vec<String>) {
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
}
