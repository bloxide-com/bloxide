// Copyright 2025 Bloxide, all rights reserved
//! Scaffold a new blox following the four-layer pattern (messages → context → blox → binary).

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::utils::{generate_spec_md, to_camel_case, workspace_root_or_cwd};

pub fn new_blox(name: &str, messages: Option<&str>, context: Option<&str>) -> Result<()> {
    let root = workspace_root_or_cwd()?;
    new_blox_in(&root, name, messages, context)
}

pub(crate) fn new_blox_in(
    root: &Path,
    name: &str,
    messages: Option<&str>,
    _context: Option<&str>,
) -> Result<()> {
    let name_snake = name.to_lowercase().replace("-", "_");
    let name_camel = to_camel_case(name);

    let spec_dir = root.join("spec/bloxes");
    fs::create_dir_all(&spec_dir)?;
    let spec_path = spec_dir.join(format!("{}.md", name_snake));
    fs::write(&spec_path, generate_spec_md(root, &name_snake, &name_camel))?;
    println!("Created: {}", spec_path.display());

    create_blox_source(root, &name_snake, &name_camel, messages)?;

    println!("\nScaffolded new blox '{}'", name);
    println!("Next steps:");
    println!(
        "  1. Edit spec/bloxes/{}.md to define states and transitions",
        name_snake
    );
    println!(
        "  2. Edit bloxes/{}/blox.toml to declare the topology, context fields, and action functions",
        name_snake
    );
    println!(
        "  3. Run `cargo blox generate` — the crate materializes into target/bloxide-generated/crates/{}-blox",
        name_snake
    );

    Ok(())
}

/// Create the pure-TOML blox source: `bloxes/<name>/blox.toml` and nothing
/// else. No Cargo.toml, no src/, no workspace registration — `cargo blox
/// generate` materializes the crate into `target/bloxide-generated/`.
pub fn create_blox_source(
    root: &Path,
    name_snake: &str,
    name_camel: &str,
    messages: Option<&str>,
) -> Result<()> {
    let blox_dir = root.join("bloxes").join(name_snake);
    fs::create_dir_all(&blox_dir)?;

    let mut blox_toml = format!(
        r#"# Copyright 2025 Bloxide, all rights reserved
[actor]
name = "{name_camel}"

[package]
description = "{name_camel} actor blox — runtime-agnostic"

[package.features]
default = ["std"]
std = ["bloxide-core/std"]

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

    fs::write(blox_dir.join("blox.toml"), blox_toml)?;

    println!("Created: {}", blox_dir.display());
    Ok(())
}
