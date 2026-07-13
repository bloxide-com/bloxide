// Copyright 2025 Bloxide, all rights reserved
//! Scaffold a new actions crate.

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::utils::{to_camel_case, update_workspace_cargo_toml, WorkspaceAddition};

pub fn new_actions(name: &str) -> Result<()> {
    let name_snake = name.to_lowercase().replace("-", "_");
    let name_camel = to_camel_case(name);
    let crate_name = format!("{}-actions", name_snake);

    let crate_dir = Path::new("crates/actions").join(&crate_name);
    let src_dir = crate_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let cargo_toml = format!(
        r#"# Copyright 2025 Bloxide, all rights reserved
[package]
name = "{crate_name}"
version.workspace = true
edition.workspace = true
description = "Action traits and generic functions for {name_camel}"
repository.workspace = true
license.workspace = true

[dependencies]
bloxide-macros = {{ workspace = true }}
"#
    );
    fs::write(crate_dir.join("Cargo.toml"), cargo_toml)?;

    let lib_rs = format!(
        r#"// Copyright 2025 Bloxide, all rights reserved
//! Action traits and generic functions for {name_camel}.
#![no_std]

use bloxide_macros::delegatable;

pub mod prelude {{
    pub use crate::*;
}}

/// Placeholder behavior trait.
#[delegatable]
pub trait CountsTicks {{
    type Count: Copy + PartialOrd + core::ops::Add<Output = Self::Count> + From<u8>;
    fn count(&self) -> Self::Count;
    fn set_count(&mut self, count: Self::Count);
}}
"#
    );
    fs::write(src_dir.join("lib.rs"), lib_rs)?;

    let member_path = format!("crates/actions/{}", crate_name);
    let dep_toml_line = format!(
        r#"{} = {{ path = "crates/actions/{}" }}"#,
        crate_name, crate_name
    );
    update_workspace_cargo_toml(&[
        WorkspaceAddition::Member(member_path),
        WorkspaceAddition::Dependency {
            name: crate_name,
            toml_line: dep_toml_line,
        },
    ])?;

    println!("Created: {}", crate_dir.display());
    Ok(())
}
