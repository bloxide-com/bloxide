// Copyright 2025 Bloxide, all rights reserved
//! `cargo blox init` — bootstrap a new bloxide workspace (#116).
//!
//! Creates a fresh workspace directory with the four-layer crate layout,
//! spec skeleton, the portable building-with-bloxide skill, an AGENTS.md,
//! and a runnable hello-world app (scaffolded via the same code path as
//! `cargo blox new-all`).
//!
//! Bloxide crates are wired as path dependencies to the bloxide checkout
//! this CLI runs from.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::new_all::new_all;
use crate::utils::{find_workspace_root_from, validate_runtime};

/// Bloxide workspace crates a fresh app workspace depends on (path deps).
const BLOXIDE_CRATES: [(&str, &str); 8] = [
    ("bloxide-core", "crates/bloxide-core"),
    ("bloxide-log", "crates/bloxide-log"),
    ("bloxide-macros", "crates/bloxide-macros"),
    ("bloxide-timer", "crates/bloxide-timer"),
    (
        "bloxide-child-management",
        "crates/bloxide-child-management",
    ),
    ("bloxide-spawn", "crates/bloxide-spawn"),
    ("bloxide-supervisor", "crates/bloxide-supervisor"),
    ("bloxide-peers", "crates/bloxide-peers"),
];

const RUNTIME_CRATES: [(&str, &str); 3] = [
    ("bloxide-tokio", "runtimes/bloxide-tokio"),
    ("bloxide-embassy", "runtimes/bloxide-embassy"),
    ("bloxide-test-runtime", "runtimes/bloxide-test-runtime"),
];

pub fn init(dir: &str, runtime: &str) -> Result<()> {
    validate_runtime(runtime)?;

    let target = Path::new(dir);
    if target.exists() && target.read_dir()?.next().is_some() {
        bail!(crate::exit::conflict(format!(
            "target directory '{}' already exists and is not empty",
            target.display()
        )));
    }

    // The bloxide checkout this CLI runs from (for path dependencies).
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let bloxide_root = find_workspace_root_from(&manifest_dir)
        .ok_or_else(|| anyhow::anyhow!("could not locate the bloxide workspace root"))?;
    let bloxide_root = bloxide_root
        .canonicalize()
        .context("failed to canonicalize bloxide root")?;

    create_workspace_layout(target, runtime, &bloxide_root)?;
    copy_spec_and_skills(target, &bloxide_root)?;
    write_agents_md(target)?;

    // Scaffold the hello-world app inside the new workspace.
    let original_dir = std::env::current_dir()?;
    std::env::set_current_dir(target).context("failed to enter new workspace")?;
    let result = new_all("counter", runtime);
    std::env::set_current_dir(original_dir)?;
    result?;

    println!("\nInitialized bloxide workspace at {}", target.display());
    println!("Next steps:");
    println!("  cd {}", target.display());
    println!("  cargo blox run --example counter");
    Ok(())
}

fn create_workspace_layout(target: &Path, runtime: &str, bloxide_root: &Path) -> Result<()> {
    for d in [
        "crates/messages",
        "crates/context",
        "bloxes",
        "crates/impl",
        "examples",
        "spec/architecture",
        "spec/bloxes",
        "spec/templates",
    ] {
        fs::create_dir_all(target.join(d))?;
    }

    let mut deps = String::new();
    for (name, rel) in BLOXIDE_CRATES.iter().chain(RUNTIME_CRATES.iter()) {
        deps.push_str(&format!(
            "{} = {{ path = \"{}\" }}\n",
            name,
            bloxide_root.join(rel).display()
        ));
    }
    if runtime == "tokio" {
        deps.push_str(
            "tokio = { version = \"1\", features = [\"full\"] }\n\
             tracing = \"0.1\"\n\
             tracing-subscriber = { version = \"0.3\", features = [\"env-filter\"] }\n\
             tracing-log = \"0.2\"\n",
        );
    } else {
        deps.push_str(
            "embassy-executor = { version = \"0.9\", features = [\"arch-std\", \"executor-thread\"] }\n\
             embassy-sync = \"0.7\"\n\
             embassy-time = { version = \"0.5\", features = [\"std\", \"generic-queue-8\"] }\n",
        );
    }

    let cargo_toml = format!(
        r#"# Copyright 2025 Bloxide, all rights reserved
[workspace]
members = [
]
resolver = "2"

[workspace.package]
version = "0.0.1"
edition = "2021"
license = "MIT"
# Set this to your project's repository — scaffolded crates inherit it.
repository = "https://example.com/your/project"

[workspace.dependencies]
{deps}"#
    );
    fs::write(target.join("Cargo.toml"), cargo_toml)?;
    Ok(())
}

fn copy_spec_and_skills(target: &Path, bloxide_root: &Path) -> Result<()> {
    // The blox-spec template is the starting point for new bloxes.
    let template_src = bloxide_root.join("spec/templates/blox-spec.md");
    if template_src.exists() {
        fs::copy(&template_src, target.join("spec/templates/blox-spec.md"))?;
    }

    fs::write(
        target.join("spec/README.md"),
        "# Blox Specs\n\nSpec-driven development: write the spec before the code.\n\
         Copy `templates/blox-spec.md` to `bloxes/<name>.md` to start a new blox.\n",
    )?;

    // The building-with-bloxide skill is portable — downstream projects get
    // their own copy (see bloxide AGENTS.md → Skills).
    let skill_src = bloxide_root.join("skills/building-with-bloxide");
    let skill_dst = target.join("skills/building-with-bloxide");
    if skill_src.exists() {
        fs::create_dir_all(&skill_dst)?;
        for entry in fs::read_dir(&skill_src)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                fs::copy(entry.path(), skill_dst.join(entry.file_name()))?;
            }
        }
    }
    Ok(())
}

fn write_agents_md(target: &Path) -> Result<()> {
    let content = r#"# Agent Orientation: <project>

A bloxide application workspace (HSM + actor messaging, runtime-agnostic bloxes).

## Read First

1. This file
2. `skills/building-with-bloxide/SKILL.md` — the end-to-end build workflow
3. `spec/README.md` — spec-driven development (spec before code)

## Layout

```
spec/                 ← blox specs (source of truth for design)
bloxes/               ← pure-TOML blox sources (blox.toml → codegen)
crates/
  messages/           ← shared plain-data message enums
  context/            ← domain context crates (action functions)
  impl/               ← impl crates (concrete behavior for impl_required actions)
examples/             ← system.toml wiring manifests (the example crate is materialized)
```

## Rules

- Spec first: write `spec/bloxes/<name>.md` before implementing.
- Never hand-edit generated code (`src/generated/`,
  `target/bloxide-generated/`) — edit `blox.toml` / `system.toml` and run
  `cargo blox generate` (it runs lint first).
- Run `cargo blox ci` before committing.
"#;
    fs::write(target.join("AGENTS.md"), content)?;
    Ok(())
}
