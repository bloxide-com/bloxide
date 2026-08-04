// Copyright 2025 Bloxide, all rights reserved
//! Scaffold a new blox following the four-layer pattern (messages → context → blox → binary).

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::utils::{
    generate_spec_md, to_camel_case, update_workspace_cargo_toml, workspace_root_or_cwd,
    WorkspaceAddition,
};

pub fn new_blox(name: &str, messages: Option<&str>, context: Option<&str>) -> Result<()> {
    let root = workspace_root_or_cwd()?;
    new_blox_in(&root, name, messages, context)
}

pub(crate) fn new_blox_in(
    root: &Path,
    name: &str,
    messages: Option<&str>,
    context: Option<&str>,
) -> Result<()> {
    let name_snake = name.to_lowercase().replace("-", "_");
    let name_camel = to_camel_case(name);

    let spec_dir = root.join("spec/bloxes");
    fs::create_dir_all(&spec_dir)?;
    let spec_path = spec_dir.join(format!("{}.md", name_snake));
    fs::write(&spec_path, generate_spec_md(root, &name_snake, &name_camel))?;
    println!("Created: {}", spec_path.display());

    create_blox_crate(root, &name_snake, &name_camel, messages, context)?;

    let member_path = format!("crates/bloxes/{}", name_snake);
    let dep_name = format!("{}-blox", name_snake);
    let dep_toml_line = format!(
        r#"{} = {{ path = "crates/bloxes/{}" }}"#,
        dep_name, name_snake
    );
    update_workspace_cargo_toml(
        root,
        &[
            WorkspaceAddition::Member(member_path),
            WorkspaceAddition::Dependency {
                name: dep_name,
                toml_line: dep_toml_line,
            },
        ],
    )?;

    println!("\nScaffolded new blox '{}'", name);
    println!("Next steps:");
    println!(
        "  1. Edit spec/bloxes/{}.md to define states and transitions",
        name_snake
    );
    println!(
        "  2. Edit crates/bloxes/{}/blox.toml to declare the topology, context fields, and action functions",
        name_snake
    );
    println!("  3. Run `cargo blox generate` to generate boilerplate");

    Ok(())
}

pub fn create_blox_crate(
    root: &Path,
    name_snake: &str,
    name_camel: &str,
    messages: Option<&str>,
    context: Option<&str>,
) -> Result<()> {
    let crate_dir = root.join("crates/bloxes").join(name_snake);
    let src_dir = crate_dir.join("src");
    let gen_dir = src_dir.join("generated");
    fs::create_dir_all(&gen_dir)?;

    let mut deps = String::from(
        r#"bloxide-core = { workspace = true }
bloxide-macros = { workspace = true }
"#,
    );
    if let Some(msg) = messages {
        deps.push_str(&format!("{} = {{ workspace = true }}\n", msg));
    }
    if let Some(ctx) = context {
        deps.push_str(&format!("{} = {{ workspace = true }}\n", ctx));
    }

    let cargo_toml = format!(
        r#"# Copyright 2025 Bloxide, all rights reserved
[package]
name = "{name_snake}-blox"
version.workspace = true
edition.workspace = true
description = "{name_camel} actor blox — runtime-agnostic"
repository.workspace = true
license.workspace = true

[features]
default = ["std"]
std = ["bloxide-core/std"]

[dependencies]
{deps}"#
    );
    fs::write(crate_dir.join("Cargo.toml"), cargo_toml)?;

    let mut blox_toml = format!(
        r#"# Copyright 2025 Bloxide, all rights reserved
[actor]
name = "{name_camel}"

[event]
name = "{name_camel}Event"
"#
    );

    // If messages crate provided, auto-add a mailbox
    if let Some(msg_crate) = messages {
        let msg_type = to_camel_case(msg_crate.trim_end_matches("-messages")) + "Msg";
        let msg_module = msg_crate.replace("-", "_");
        blox_toml.push_str(&format!(
            r#"
[[event.mailboxes]]
variant = "Msg"
message = "{msg_type}"
message_path = "{msg_module}::{msg_type}"
"#
        ));
    }

    // Minimal [context] section — self_id is auto-emitted by the codegen,
    // state fields go in [[context.fields]], refs in [[context.uses]].
    // One initial leaf state so the scaffold builds end-to-end.
    blox_toml.push_str(&format!(
        r#"
[context]
name = "{name_camel}Ctx"

[topology]
[[topology.states]]
name = "Ready"
initial = true
"#
    ));

    // With a messages crate, add a Tick → done transition so the scaffold
    // runs and exits cleanly on the bootstrap Tick.
    if let Some(msg_crate) = messages {
        let msg_type = to_camel_case(msg_crate.trim_end_matches("-messages")) + "Msg";
        blox_toml.push_str(&format!(
            r#"
[[topology.transitions]]
state = "Ready"
event = "{msg_type}::Tick(_)"
target = "done"
"#
        ));
    }

    fs::write(crate_dir.join("blox.toml"), blox_toml)?;

    let lib_rs = r#"// Copyright 2025 Bloxide, all rights reserved
#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod generated;
pub mod prelude;

#[cfg(all(test, feature = "std"))]
mod tests;

pub use generated::*;
"#;
    fs::write(src_dir.join("lib.rs"), lib_rs)?;

    let prelude_rs = format!(
        r#"// Copyright 2025 Bloxide, all rights reserved
pub use crate::{{{name_camel}Ctx, {name_camel}Event, {name_camel}Spec, {name_camel}State}};
"#
    );
    fs::write(src_dir.join("prelude.rs"), prelude_rs)?;

    let tests_rs = format!(
        r#"// Copyright 2025 Bloxide, all rights reserved
//! Unit tests for the {name_snake} blox (TestRuntime — see ping/src/tests.rs).
"#
    );
    fs::write(src_dir.join("tests.rs"), tests_rs)?;

    let gen_mod_rs =
        "// Auto-generated module.\n// Files in this directory are generated by bloxide-codegen.\n";
    fs::write(gen_dir.join("mod.rs"), gen_mod_rs)?;

    println!("Created: {}", crate_dir.display());
    Ok(())
}
