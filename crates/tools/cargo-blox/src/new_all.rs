// Copyright 2025 Bloxide, all rights reserved
//! Convenience command: scaffold all layers at once.

use anyhow::Result;

use crate::new::new_blox;
use crate::new_binary::new_binary;
use crate::new_context::new_context;
use crate::new_messages::new_messages;

pub fn new_all(name: &str, runtime: &str) -> Result<()> {
    let name_snake = name.to_lowercase().replace("-", "_");
    let msg_crate = format!("{}-messages", name_snake);
    let ctx_crate = format!("blox-ctx-{}", name_snake);

    new_messages(name)?;
    new_context(name)?;
    new_blox(name, Some(&msg_crate), Some(&ctx_crate))?;
    new_binary(name, runtime)?;

    println!("\nScaffolded all layers for '{}'", name);
    Ok(())
}
