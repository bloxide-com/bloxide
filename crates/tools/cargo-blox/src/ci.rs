// Copyright 2025 Bloxide, all rights reserved
//! Run full CI feature matrix.

use std::process::Command;

pub fn ci() -> anyhow::Result<()> {
    let checks: Vec<(&str, Vec<&str>)> = vec![
        ("bloxide-core", vec!["--no-default-features"]),
        (
            "bloxide-core",
            vec!["--no-default-features", "--features", "alloc"],
        ),
        ("bloxide-core", vec!["--features", "std"]),
        (
            "bloxide-timer",
            vec!["--target", "riscv32imc-unknown-none-elf"],
        ),
        ("ping-pong-messages", vec!["--no-default-features"]),
    ];

    // no_std audit matrix (issue #37): every platform + messages crate must
    // compile with no default features against a bare-metal target.
    let nostd_audit = [
        "bloxide-core",
        "bloxide-log",
        "bloxide-timer",
        "bloxide-supervisor",
        "bloxide-spawn",
        "bloxide-child-management",
        "bloxide-peers",
        "ping-pong-messages",
        "pool-messages",
        "counter-messages",
        "bhsm-tst-messages",
    ];

    let mut failed = 0;
    for (pkg, args) in &checks {
        println!();
        println!("========================================");
        println!("  cargo check -p {} {}", pkg, args.join(" "));
        println!("========================================");
        let status = Command::new("cargo")
            .arg("check")
            .arg("-p")
            .arg(pkg)
            .args(args)
            .status()?;
        if !status.success() {
            eprintln!("FAILED: cargo check -p {} {}", pkg, args.join(" "));
            failed += 1;
        } else {
            println!("OK: cargo check -p {} {}", pkg, args.join(" "));
        }
    }

    for pkg in nostd_audit {
        let args = [
            "--no-default-features",
            "--target",
            "riscv32imc-unknown-none-elf",
        ];
        println!();
        println!("========================================");
        println!("  cargo check -p {} {}", pkg, args.join(" "));
        println!("========================================");
        let status = Command::new("cargo")
            .arg("check")
            .arg("-p")
            .arg(pkg)
            .args(args)
            .status()?;
        if !status.success() {
            eprintln!("FAILED: cargo check -p {} {}", pkg, args.join(" "));
            failed += 1;
        } else {
            println!("OK: cargo check -p {} {}", pkg, args.join(" "));
        }
    }

    // Test suites (mirrors scripts/ci.sh): bloxide-core needs --features std
    // for its cfg-gated tests, then the full workspace with default features.
    for test_args in [
        vec!["test", "-p", "bloxide-core", "--features", "std"],
        vec!["test"],
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
