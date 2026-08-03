// Copyright 2025 Bloxide, all rights reserved
//! Watch and regenerate on changes.

use clap_cargo::Features;
use notify::{RecursiveMode, Watcher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn watch(cargo: Features) -> anyhow::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;

    let root = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    watcher.watch(&root, RecursiveMode::Recursive)?;

    println!("bloxide: watching for blox.toml and system.toml changes...");

    let mut last_regen = Instant::now();

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                // Skip build-output churn — nothing under target/ is a source.
                let in_target = |p: &std::path::Path| {
                    p.components()
                        .any(|c| c.as_os_str() == std::ffi::OsStr::new("target"))
                };
                // Both blox.toml (per-blox codegen) and system.toml (app
                // wiring: main.rs + Cargo.toml) trigger regeneration —
                // generate() handles both paths.
                if event.paths.iter().any(|p| {
                    !in_target(p)
                        && p.file_name().is_some_and(|n| {
                            n == std::ffi::OsStr::new("blox.toml")
                                || n == std::ffi::OsStr::new("system.toml")
                        })
                }) {
                    let now = Instant::now();
                    if now.duration_since(last_regen) >= Duration::from_millis(500) {
                        last_regen = now;
                        for path in &event.paths {
                            println!("bloxide: change detected in {}", path.display());
                        }
                        if let Err(e) = crate::generate::generate(Some(root.clone())) {
                            eprintln!("bloxide: generate failed: {}", e);
                        }
                        let cargo_bin =
                            std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
                        let mut check_args = vec!["check".to_string()];
                        if cargo.all_features {
                            check_args.push("--all-features".to_string());
                        }
                        if cargo.no_default_features {
                            check_args.push("--no-default-features".to_string());
                        }
                        if !cargo.features.is_empty() {
                            check_args.push("--features".to_string());
                            check_args.push(cargo.features.join(" "));
                        }
                        let status = std::process::Command::new(&cargo_bin)
                            .args(&check_args)
                            .status()?;
                        if !status.success() {
                            eprintln!("bloxide: cargo check failed");
                        } else {
                            println!("bloxide: cargo check succeeded");
                        }
                    }
                }
            }
            Ok(Err(e)) => eprintln!("bloxide: watch error: {}", e),
            Err(_) => break,
        }
    }

    Ok(())
}
