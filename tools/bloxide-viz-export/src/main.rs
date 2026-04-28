// Copyright 2025 Bloxide, all rights reserved
use bloxide_viz_export::{export_workspace, write_specs_to_json};
use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!(
            "Usage: {} <path-to-bloxide-workspace> [output-dir]",
            args[0]
        );
        eprintln!("");
        eprintln!("Scans the workspace for blox crates and exports visualization JSON files.");
        eprintln!("If output-dir is not provided, outputs to ./bloxide-viz-output/");
        std::process::exit(1);
    }

    let workspace_path = Path::new(&args[1]);
    let output_dir = args
        .get(2)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("bloxide-viz-output"));

    if !workspace_path.exists() {
        eprintln!("Error: path '{}' does not exist", workspace_path.display());
        std::process::exit(1);
    }

    println!("Scanning {} for blox crates...", workspace_path.display());

    match export_workspace(workspace_path) {
        Ok(specs) => {
            println!("Found {} blox crate(s):", specs.len());
            for spec in &specs {
                println!("  - {}", spec.name);
            }

            if let Err(e) = write_specs_to_json(&specs, &output_dir) {
                eprintln!("Error writing JSON: {}", e);
                std::process::exit(1);
            }

            println!(
                "\nDone. Exported {} blox spec(s) to {}",
                specs.len(),
                output_dir.display()
            );
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
