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
