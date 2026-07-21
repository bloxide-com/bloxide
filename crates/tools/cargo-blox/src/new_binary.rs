// Copyright 2025 Bloxide, all rights reserved
//! Scaffold a new binary (wiring) crate.

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::utils::{to_camel_case, update_workspace_cargo_toml, WorkspaceAddition};

pub fn new_binary(name: &str, runtime: &str) -> Result<()> {
    let name_snake = name.to_lowercase().replace("-", "_");
    let name_camel = to_camel_case(name);

    let apps_dir = Path::new("apps");
    let crate_dir = apps_dir.join(&name_snake);
    let src_dir = crate_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let (runtime_dep, main_rs) = match runtime {
        "embassy" => (
            "bloxide-embassy = { workspace = true }\nembassy-executor = { version = \"0.9\", features = [\"arch-std\", \"executor-thread\"] }",
            format!(
                r#"// Copyright 2025 Bloxide, all rights reserved
//! {name_camel} wiring binary — Embassy runtime.

use embassy_executor::Spawner;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {{
}}
"#
            ),
        ),
        _ => (
            "bloxide-tokio = { workspace = true }\ntokio = { workspace = true }",
            format!(
                r#"// Copyright 2025 Bloxide, all rights reserved
//! {name_camel} wiring binary — Tokio runtime.
#[tokio::main]
async fn main() {{
}}
"#
            ),
        ),
    };

    let cargo_toml = format!(
        r#"# Copyright 2025 Bloxide, all rights reserved
[package]
name = "{name_snake}"
version.workspace = true
edition.workspace = true
publish = false

[dependencies]
bloxide-core = {{ workspace = true }}
{runtime_dep}
"#
    );
    fs::write(crate_dir.join("Cargo.toml"), cargo_toml)?;
    fs::write(src_dir.join("main.rs"), main_rs)?;

    let member_path = format!("apps/{}", name_snake);
    update_workspace_cargo_toml(&[WorkspaceAddition::Member(member_path)])?;

    println!("Created: {}", crate_dir.display());
    Ok(())
}
