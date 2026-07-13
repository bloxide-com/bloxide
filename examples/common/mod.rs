// Copyright 2025 Bloxide, all rights reserved
//! Shared helpers for CLI-driven example scripts.
//!
//! Each example uses `cargo-blox` to scaffold a demo workspace, then writes
//! custom user-edited files, generates boilerplate, and builds the result.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Locate the repo root (parent of the `examples/` directory).
pub fn repo_root() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    exe.ancestors()
        .nth(2)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".."))
}

/// Build `cargo-blox` in debug mode and return the path to the binary.
pub fn ensure_cargo_blox(root: &Path) -> PathBuf {
    let status = Command::new("cargo")
        .args(["build", "-p", "cargo-blox", "--quiet"])
        .current_dir(root)
        .status()
        .expect("failed to run `cargo build -p cargo-blox`");
    assert!(status.success(), "cargo-blox failed to build");

    root.join("target/debug/cargo-blox")
}

/// Create a fresh demo workspace directory under `demo/<name>`.
/// Removes any existing directory first.
pub fn create_demo_dir(root: &Path, name: &str) -> PathBuf {
    let dir = root.join("demo").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("failed to remove old demo dir");
    }
    fs::create_dir_all(&dir).expect("failed to create demo dir");
    dir
}

/// Run `cargo-blox blox <args>` in the given workspace directory.
pub fn blox(binary: &Path, workspace: &Path, args: &[&str]) {
    let status = Command::new(binary)
        .args(["blox"])
        .args(args)
        .current_dir(workspace)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .unwrap_or_else(|e| panic!("failed to run cargo-blox blox {args:?}: {e}"));
    assert!(
        status.success(),
        "cargo-blox blox {args:?} failed with status {status}"
    );
}

/// Write `content` to `workspace/relative_path`, creating parent dirs as needed.
pub fn write_file(workspace: &Path, relative: &str, content: &str) {
    let path = workspace.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create parent dirs");
    }
    fs::write(&path, content).unwrap_or_else(|e| panic!("failed to write {path:?}: {e}"));
}

/// Append `content` to `workspace/relative_path`.
pub fn append_file(workspace: &Path, relative: &str, content: &str) {
    let path = workspace.join(relative);
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .unwrap_or_else(|e| panic!("failed to open {path:?}: {e}"));
    file.write_all(content.as_bytes())
        .unwrap_or_else(|e| panic!("failed to append to {path:?}: {e}"));
}

/// Remove a file if it exists.
pub fn remove_file(workspace: &Path, relative: &str) {
    let path = workspace.join(relative);
    if path.exists() {
        fs::remove_file(&path).unwrap_or_else(|e| panic!("failed to remove {path:?}: {e}"));
    }
}

/// Run `cargo run -p <pkg>` in the given directory.
pub fn cargo_run(dir: &Path, pkg: &str) {
    let status = Command::new("cargo")
        .args(["run", "-p", pkg])
        .current_dir(dir)
        .status()
        .expect("failed to run cargo run");
    assert!(status.success(), "cargo run -p {pkg} failed");
}
