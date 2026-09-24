// Copyright 2025 Bloxide, all rights reserved
//! Run full CI feature matrix.
//!
//! The check matrix is discovered from the workspace, not hardcoded:
//!
//! - `cargo check --workspace` covers every member with default features.
//! - Feature combos are derived per member crate: an `alloc` feature earns
//!   `--no-default-features` (and `--features alloc` when alloc is not in
//!   the default set); a `std` feature earns `--features std`.
//! - The no_std audit matrix comes from `[workspace.metadata.bloxide-ci]`
//!   in the root Cargo.toml (`nostd-target` + `nostd-members`) — new
//!   platform crates are added there, not in this file.
//!
//! Tests, fmt, clippy, doc build, and the copyright check are generic.

use std::process::Command;

/// A workspace member with its package name and declared features.
struct Member {
    name: String,
    features: Vec<String>,
    default_features: Vec<String>,
}

pub fn ci() -> anyhow::Result<()> {
    let members = discover_members()?;
    let (nostd_target, nostd_members) = nostd_audit_config();

    let mut jobs: Vec<(String, Vec<String>)> = Vec::new();

    // Workspace-wide default check covers all members at once.
    jobs.push(("--workspace".to_string(), Vec::new()));

    // Per-member feature combos, derived from each crate's [features] table.
    for m in &members {
        let has = |f: &str| m.features.iter().any(|x| x == f);
        if has("alloc") {
            jobs.push((m.name.clone(), vec!["--no-default-features".to_string()]));
            if !m.default_features.iter().any(|x| x == "alloc") {
                jobs.push((
                    m.name.clone(),
                    vec![
                        "--no-default-features".to_string(),
                        "--features".to_string(),
                        "alloc".to_string(),
                    ],
                ));
            }
        }
        if has("std") {
            jobs.push((
                m.name.clone(),
                vec!["--features".to_string(), "std".to_string()],
            ));
        }
    }

    // no_std audit matrix (issue #37), from workspace metadata.
    if let (Some(target), Some(audit)) = (&nostd_target, &nostd_members) {
        for pkg in audit {
            jobs.push((
                pkg.clone(),
                vec![
                    "--no-default-features".to_string(),
                    "--target".to_string(),
                    target.clone(),
                ],
            ));
        }
    } else {
        eprintln!(
            "bloxide: warning: no [workspace.metadata.bloxide-ci] nostd-target/nostd-members — skipping no_std audit"
        );
    }

    let mut failed = 0;
    for (pkg, args) in &jobs {
        println!();
        println!("========================================");
        println!("  cargo check -p {} {}", pkg, args.join(" "));
        println!("========================================");
        let status = if pkg == "--workspace" {
            Command::new("cargo")
                .arg("check")
                .arg("--workspace")
                .status()?
        } else {
            Command::new("cargo")
                .arg("check")
                .arg("-p")
                .arg(pkg)
                .args(args)
                .status()?
        };
        if !status.success() {
            eprintln!("FAILED: cargo check -p {} {}", pkg, args.join(" "));
            failed += 1;
        } else {
            println!("OK: cargo check -p {} {}", pkg, args.join(" "));
        }
    }

    // Lint: friendly TOML validation across all blox.toml files (#114/#122).
    println!();
    println!("========================================");
    println!("  cargo blox lint");
    println!("========================================");
    let status = Command::new("cargo")
        .args(["run", "-p", "cargo-blox", "--quiet", "--", "blox", "lint"])
        .status()?;
    if !status.success() {
        eprintln!("FAILED: cargo blox lint");
        failed += 1;
    } else {
        println!("OK: cargo blox lint");
    }

    // Test suites: bloxide-core and bloxide-embassy
    // need --features std for their cfg-gated tests, then the full workspace
    // and generated workspaces plus standalone impl crates with default features.
    // bloxide-embassy is tested in isolation because
    // workspace-level feature unification masks its host link requirements
    // (critical-section std impl) that only surface with `-p`.
    for test_args in [
        vec!["test", "-p", "bloxide-core", "--features", "std"],
        vec!["test", "-p", "bloxide-embassy", "--features", "std"],
        vec!["run", "-p", "cargo-blox", "--quiet", "--", "blox", "test"],
    ] {
        println!();
        println!("========================================");
        println!("  cargo {}", test_args.join(" "));
        println!("========================================");
        let status = Command::new("cargo").args(&test_args).status()?;
        if !status.success() {
            eprintln!("FAILED: cargo {}", test_args.join(" "));
            failed += 1;
        } else {
            println!("OK: cargo {}", test_args.join(" "));
        }
    }

    // Format check
    println!();
    println!("========================================");
    println!("  cargo fmt --check");
    println!("========================================");
    let status = Command::new("cargo")
        .args(["fmt", "--", "--check"])
        .status()?;
    if !status.success() {
        eprintln!("FAILED: cargo fmt --check");
        failed += 1;
    } else {
        println!("OK: cargo fmt --check");
    }

    // Clippy
    println!();
    println!("========================================");
    println!("  cargo clippy --all-targets");
    println!("========================================");
    let status = Command::new("cargo")
        .args([
            "clippy",
            "--all-targets",
            "--",
            "-W",
            "warnings",
            "-D",
            "warnings",
        ])
        .status()?;
    if !status.success() {
        eprintln!("FAILED: cargo clippy");
        failed += 1;
    } else {
        println!("OK: cargo clippy");
    }

    // Doc build with warnings as errors (mirrors scripts/ci.sh).
    println!();
    println!("========================================");
    println!("  cargo doc --workspace --no-deps");
    println!("========================================");
    let status = Command::new("cargo")
        .args(["doc", "--workspace", "--no-deps"])
        .env("RUSTDOCFLAGS", "-Dwarnings")
        .status()?;
    if !status.success() {
        eprintln!("FAILED: cargo doc --workspace --no-deps");
        failed += 1;
    } else {
        println!("OK: cargo doc --workspace --no-deps");
    }

    // Copyright header check (mirrors scripts/ci.sh).
    println!();
    println!("========================================");
    println!("  Copyright Compliance");
    println!("========================================");
    let missing = find_missing_copyright();
    if !missing.is_empty() {
        eprintln!("FAILED: incorrect copyright notice found:");
        for f in &missing {
            eprintln!("  {}", f);
        }
        failed += 1;
    } else {
        println!("OK: all source files have correct copyright notices");
    }

    // Dependency audit (cargo-deny, issue #39). Optional tool: warn and skip
    // when not installed rather than failing the local run.
    println!();
    println!("========================================");
    println!("  cargo deny check");
    println!("========================================");
    let deny_available = Command::new("cargo")
        .args(["deny", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !deny_available {
        println!("SKIP: cargo-deny not installed (cargo install cargo-deny --locked)");
    } else {
        let status = Command::new("cargo").args(["deny", "check"]).status()?;
        if !status.success() {
            eprintln!("FAILED: cargo deny check");
            failed += 1;
        } else {
            println!("OK: cargo deny check");
        }
    }

    println!();
    println!("========================================");
    if failed == 0 {
        println!("All CI checks passed!");
        println!("========================================");
        Ok(())
    } else {
        println!("{} CI check(s) failed!", failed);
        println!("========================================");
        anyhow::bail!("{} CI checks failed", failed)
    }
}

/// Discover workspace members from the root Cargo.toml: package names and
/// each crate's declared `[features]` / default set.
fn discover_members() -> anyhow::Result<Vec<Member>> {
    let root_toml = std::fs::read_to_string("Cargo.toml")?;
    let root: toml::Value = toml::from_str(&root_toml)?;
    let member_paths = root
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or_else(|| anyhow::anyhow!("no [workspace] members in root Cargo.toml"))?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<Vec<_>>();

    let mut members = Vec::new();
    for path in member_paths {
        let manifest = std::path::Path::new(&path).join("Cargo.toml");
        let Ok(content) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let parsed: toml::Value = toml::from_str(&content)?;
        let name = parsed
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(str::to_string);
        let features = parsed
            .get("features")
            .and_then(|f| f.as_table())
            .map(|t| t.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let default_features = parsed
            .get("features")
            .and_then(|f| f.get("default"))
            .and_then(|d| d.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(name) = name {
            members.push(Member {
                name,
                features,
                default_features,
            });
        }
    }
    Ok(members)
}

/// Read `[workspace.metadata.bloxide-ci]` from the root Cargo.toml.
fn nostd_audit_config() -> (Option<String>, Option<Vec<String>>) {
    let Ok(content) = std::fs::read_to_string("Cargo.toml") else {
        return (None, None);
    };
    let Ok(parsed) = toml::from_str::<toml::Value>(&content) else {
        return (None, None);
    };
    let meta = parsed
        .get("workspace")
        .and_then(|w| w.get("metadata"))
        .and_then(|m| m.get("bloxide-ci"));
    let target = meta
        .and_then(|c| c.get("nostd-target"))
        .and_then(|t| t.as_str())
        .map(str::to_string);
    let members = meta
        .and_then(|c| c.get("nostd-members"))
        .and_then(|m| m.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        });
    (target, members)
}

/// Walk the workspace for `.rs` / `Cargo.toml` files (excluding `target/`)
/// that lack the Bloxide copyright header. Mirrors scripts/ci.sh.
fn find_missing_copyright() -> Vec<String> {
    let mut missing = Vec::new();
    let pattern = "Copyright 202";
    let mut stack = vec![std::path::PathBuf::from(".")];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    stack.push(path);
                }
            } else if name.ends_with(".rs") || name == "Cargo.toml" {
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let has_header = content
                    .lines()
                    .take(3)
                    .any(|l| l.contains(pattern) && l.contains("Bloxide, all rights reserved"));
                if !has_header {
                    missing.push(path.display().to_string());
                }
            }
        }
    }
    missing.sort();
    missing
}
