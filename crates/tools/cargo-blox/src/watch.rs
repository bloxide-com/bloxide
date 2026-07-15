// Copyright 2025 Bloxide, all rights reserved
//! Watch and regenerate on changes.

use clap_cargo::Features;
use notify::{RecursiveMode, Watcher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn watch(_cargo: Features) -> anyhow::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;

    let root = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    watcher.watch(&root, RecursiveMode::Recursive)?;

    println!("bloxide: watching for blox.toml changes...");

    let mut last_regen = Instant::now();

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                if event.paths.iter().any(|p| {
                    p.file_name()
                        .is_some_and(|n| n == std::ffi::OsStr::new("blox.toml"))
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
                        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
                        let status = std::process::Command::new(&cargo)
                            .args(["check"])
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
