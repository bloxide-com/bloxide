// Copyright 2025 Bloxide, all rights reserved
//! Convenience command: scaffold all layers at once.

use anyhow::Result;

use crate::new::new_blox;
use crate::new_binary::new_binary;
use crate::new_context::new_context;
use crate::new_impl::new_impl;
use crate::new_messages::new_messages;

pub fn new_all(name: &str, runtime: &str) -> Result<()> {
    let name_snake = name.to_lowercase().replace("-", "_");
    let msg_crate = format!("{}-messages", name_snake);
    let ctx_crate = format!("blox-ctx-{}", name_snake);

    // 1. messages crate (shared plain-data enums)
    new_messages(name)?;
    // 2. context crate (domain action functions)
    new_context(name)?;
    // 3. blox crate (declarative topology + spec/bloxes/<name>.md)
    new_blox(name, Some(&msg_crate), Some(&ctx_crate))?;
    // 4. impl crate (concrete behavior for impl_required actions)
    new_impl(&name_snake, &name_snake)?;
    // 5. app: system.toml wiring manifest
    new_binary(name, runtime)?;
    // 6. generate blox boilerplate + app main.rs + Cargo.toml
    crate::generate::generate(None)?;

    println!("\nScaffolded all layers for '{}'", name);
    println!("Next steps:");
    println!(
        "  1. Edit spec/bloxes/{}.md to define states and transitions",
        name_snake
    );
    println!(
        "  2. Edit crates/bloxes/{}/blox.toml to declare the topology",
        name_snake
    );
    println!("  3. cargo blox run -- -p {}", name_snake);
    Ok(())
}
