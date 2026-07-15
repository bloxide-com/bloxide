// Copyright 2025 Bloxide, all rights reserved
//! CLI binary for `bloxide-codegen`.
//!
//! Usage: `bloxide-codegen <input-toml> <output-dir>`

use bloxide_codegen::generate_from_toml;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: bloxide-codegen <input-toml> <output-dir>");
        std::process::exit(1);
    }

    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);

    std::fs::create_dir_all(output)?;

    let files = generate_from_toml(input)?;
    for (filename, content) in files {
        let path = output.join(&filename);
        std::fs::write(&path, content)?;
        eprintln!("Generated: {}", path.display());
    }

    Ok(())
}
