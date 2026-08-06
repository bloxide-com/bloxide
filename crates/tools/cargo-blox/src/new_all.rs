// Copyright 2025 Bloxide, all rights reserved
//! Convenience command: scaffold all layers at once.

use anyhow::Result;

use crate::new::new_blox_in;
use crate::new_binary::new_binary_in;
use crate::new_context::new_context_in;
use crate::new_impl::new_impl_in;
use crate::new_messages::new_messages_in;
use crate::utils::{validate_runtime, workspace_root_or_cwd};

pub fn new_all(name: &str, runtime: &str) -> Result<()> {
    // Validate up front: the binary layer (step 5) checks too, but by then
    // layers 1-4 would already be written.
    validate_runtime(runtime)?;

    // Resolve the workspace root once and thread it through every layer so a
    // `new-all` invoked from a subdirectory doesn't mix roots.
    let root = workspace_root_or_cwd()?;
    let name_snake = name.to_lowercase().replace("-", "_");
    let msg_crate = format!("{}-messages", name_snake);
    let ctx_crate = format!("blox-ctx-{}", name_snake);

    // 1. messages crate (shared plain-data enums)
    new_messages_in(&root, name)?;
    // 2. context crate (domain action functions)
    new_context_in(&root, name)?;
    // 3. blox crate (declarative topology + spec/bloxes/<name>.md)
    new_blox_in(&root, name, Some(&msg_crate), Some(&ctx_crate))?;
    // 4. impl crate (concrete behavior for impl_required actions)
    new_impl_in(&root, &name_snake, &name_snake)?;
    // 5. example: system.toml wiring manifest
    new_binary_in(&root, name, runtime)?;
    // 6. generate blox boilerplate + materialize the example crate
    crate::generate::generate(Some(root))?;

    println!("\nScaffolded all layers for '{}'", name);
    println!("Next steps:");
    println!(
        "  1. Edit spec/bloxes/{}.md to define states and transitions",
        name_snake
    );
    println!(
        "  2. Edit bloxes/{}/blox.toml to declare the topology",
        name_snake
    );
    println!("  3. cargo blox run --example {}", name_snake);
    Ok(())
}
