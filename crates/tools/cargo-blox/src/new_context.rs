// Copyright 2025 Bloxide, all rights reserved
//! Scaffold a new context crate (free action functions, no traits).
//!
//! Note: action crates with behavior/accessor traits were eliminated.
//! Context crates are portable interface layers: plain free functions taking
//! concrete params, `#![no_std]`, no runtime imports (see AGENTS.md
//! invariant #10).

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::utils::{to_camel_case, update_workspace_cargo_toml, WorkspaceAddition};

pub fn new_context(name: &str) -> Result<()> {
    let name_snake = name.to_lowercase().replace("-", "_");
    let name_camel = to_camel_case(name);
    let crate_name = format!("blox-ctx-{}", name_snake);

    let crate_dir = Path::new("crates/context").join(&crate_name);
    let src_dir = crate_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let cargo_toml = format!(
        r#"# Copyright 2025 Bloxide, all rights reserved
[package]
name = "{crate_name}"
version.workspace = true
edition.workspace = true
description = "Context crate (action functions) for {name_camel}"
repository.workspace = true
license.workspace = true

[dependencies]
"#
    );
    fs::write(crate_dir.join("Cargo.toml"), cargo_toml)?;

    let lib_rs = format!(
        r#"// Copyright 2025 Bloxide, all rights reserved
//! Context crate for {name_camel} — free action functions taking concrete
//! params. No traits, no runtime imports, no file I/O (AGENTS.md invariant #10).
#![no_std]

pub mod prelude {{
    pub use crate::*;
}}

/// Example action function: increments a counter field by one.
/// Declare it in `[[context.actions]]` in the blox's `blox.toml` with
/// `fields = ["count:mut"]`, then wire it into a transition.
/// Action functions return `ActionResult` so guards can react to failures.
pub fn increment_count(count: &mut u32) -> bloxide_core::transition::ActionResult {{
    *count += 1;
    bloxide_core::transition::ActionResult::Ok
}}
"#
    );
    fs::write(src_dir.join("lib.rs"), lib_rs)?;

    let member_path = format!("crates/context/{}", crate_name);
    let dep_toml_line = format!(
        r#"{} = {{ path = "crates/context/{}" }}"#,
        crate_name, crate_name
    );
    update_workspace_cargo_toml(&[
        WorkspaceAddition::Member(member_path),
        WorkspaceAddition::Dependency {
            name: crate_name.clone(),
            toml_line: dep_toml_line,
        },
    ])?;

    println!("Created: {}", crate_dir.display());
    println!(
        "Context crate '{}' — declare its functions in [[context.actions]] of your blox.toml",
        crate_name
    );
    Ok(())
}
