// Copyright 2025 Bloxide, all rights reserved
//! Generate code from all blox.toml files in the workspace.

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::utils::{find_workspace_root_from, DISCOVERY_MAX_DEPTH};

pub fn generate(workspace: Option<PathBuf>) -> anyhow::Result<()> {
    let root = workspace.unwrap_or_else(|| {
        let manifest_dir =
            PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
        find_workspace_root_from(&manifest_dir).unwrap_or(manifest_dir)
    });

    // Lint before generating (issues #114/#122): invalid TOML fails fast
    // with friendly diagnostics instead of codegen errors or Rust compile
    // errors downstream.
    crate::lint::lint()?;

    let mut count = 0;
    for entry in WalkDir::new(&root)
        .max_depth(DISCOVERY_MAX_DEPTH)
        .into_iter()
        .filter_entry(|e| e.file_name() != "target")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() == "blox.toml")
    {
        let toml_path = entry.path();
        let crate_dir = toml_path.parent().unwrap();
        let src_dir = crate_dir.join("src");
        let generated_dir = src_dir.join("generated");

        let files = match bloxide_codegen::generate_from_toml(toml_path) {
            Ok(files) => files,
            Err(e) => {
                eprintln!(
                    "bloxide: warning: failed to generate for {}: {}",
                    toml_path.display(),
                    e
                );
                continue;
            }
        };

        // Nothing to emit for this crate (e.g. bloxide-core, whose
        // `[mailboxes]`-only blox.toml is consumed by its build.rs at build
        // time). Skip entirely: do not create src/generated/ or touch lib.rs.
        if files.is_empty() {
            continue;
        }

        std::fs::create_dir_all(&generated_dir)?;

        for (filename, content) in &files {
            // mod.rs is owned by `ensure_generated_mod` below (single writer —
            // two writers with different formatting would ping-pong the file).
            if filename == "mod.rs" {
                continue;
            }
            let path = generated_dir.join(filename);

            // Format the candidate before comparing so previously written
            // (already rustfmt'd) files compare equal — otherwise every run
            // rewrites every file (mtime churn) and spams "generated ...".
            let content = rustfmt_text(content);

            // Only write if changed (preserves mtime for caching)
            let needs_write = if path.exists() {
                std::fs::read_to_string(&path)? != content
            } else {
                true
            };

            if needs_write {
                std::fs::write(&path, &content)?;
                println!("bloxide: generated {}", path.display());
            }
        }

        // Ensure lib.rs includes the generated module
        ensure_generated_mod(&src_dir, &files)?;
        count += 1;
    }

    println!("bloxide: processed {} blox.toml files", count);

    // ── Process system.toml files (app wiring) ───────────────────────────
    // After generating blox crate code, also regenerate main.rs for any
    // system.toml manifests found in the workspace. This makes
    // `cargo blox build`/`check`/`test`/`run` automatically regenerate
    // app wiring alongside blox crate code — no separate `cargo blox wire`
    // step needed.
    let mut wire_count = 0;
    for entry in WalkDir::new(&root)
        .max_depth(DISCOVERY_MAX_DEPTH)
        .into_iter()
        .filter_entry(|e| e.file_name() != "target")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() == "system.toml")
    {
        let system_path = entry.path();
        let app_dir = system_path.parent().unwrap();

        // Generate main.rs (app wiring).
        match bloxide_codegen::generate_system_wiring_from_toml(system_path, &root) {
            Ok(main_rs) => {
                let output_path = app_dir.join("src").join("main.rs");
                std::fs::create_dir_all(output_path.parent().unwrap())?;

                let needs_write = if output_path.exists() {
                    std::fs::read_to_string(&output_path)? != main_rs
                } else {
                    true
                };

                if needs_write {
                    std::fs::write(&output_path, &main_rs)?;
                    println!("bloxide: generated {}", output_path.display());
                }
            }
            Err(e) => {
                eprintln!(
                    "bloxide: warning: failed to wire {}: {}",
                    system_path.display(),
                    e
                );
                continue;
            }
        }

        // Generate Cargo.toml (app dependencies).
        match bloxide_codegen::generate_cargo_toml(system_path, &root) {
            Ok(cargo_toml) => {
                let cargo_path = app_dir.join("Cargo.toml");
                let needs_write = if cargo_path.exists() {
                    // Preserve the header comment if the existing file has one
                    // that differs from our generated one. We only update
                    // if the generated content differs from the existing file.
                    std::fs::read_to_string(&cargo_path)? != cargo_toml
                } else {
                    true
                };

                if needs_write {
                    std::fs::write(&cargo_path, &cargo_toml)?;
                    println!("bloxide: generated {}", cargo_path.display());
                }
            }
            Err(e) => {
                eprintln!(
                    "bloxide: warning: failed to generate Cargo.toml for {}: {}",
                    system_path.display(),
                    e
                );
            }
        }

        wire_count += 1;
    }

    if wire_count > 0 {
        println!("bloxide: processed {} system.toml files", wire_count);
    }

    // Generated files were rustfmt'd individually before writing (see
    // `rustfmt_text`) — no whole-workspace `cargo fmt` here, which would also
    // rewrite hand-written files.

    Ok(())
}

/// Format Rust source via `rustfmt` (edition 2021). Best effort: returns the
/// input unchanged if rustfmt is unavailable or fails — the file is still
/// written, just unformatted.
fn rustfmt_text(source: &str) -> String {
    use std::io::Write;
    let mut child = match std::process::Command::new("rustfmt")
        .args(["--edition", "2021"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return source.to_string(),
    };
    if child
        .stdin
        .take()
        .map(|mut s| s.write_all(source.as_bytes()))
        .is_none()
    {
        return source.to_string();
    }
    match child.wait_with_output() {
        Ok(out) if out.status.success() => {
            String::from_utf8(out.stdout).unwrap_or_else(|_| source.to_string())
        }
        _ => source.to_string(),
    }
}

fn ensure_generated_mod(
    src_dir: &Path,
    generated_files: &[(String, String)],
) -> anyhow::Result<()> {
    let lib_rs = src_dir.join("lib.rs");
    let mod_line = "pub mod generated;\n";

    if lib_rs.exists() {
        let content = std::fs::read_to_string(&lib_rs)?;
        if !content.contains("mod generated") {
            std::fs::write(&lib_rs, format!("{}\n{}", mod_line, content))?;
        }
    }

    // Write generated/mod.rs with pub mod declarations for each generated file.
    // Only include files in the generated_files list — stale files left on disk
    // from previous generations are NOT preserved, so removing a blox from the
    // manifest correctly removes its module from mod.rs.
    let generated_mod = src_dir.join("generated").join("mod.rs");
    let mod_names: Vec<String> = generated_files
        .iter()
        .filter_map(|(filename, _)| {
            if filename.ends_with(".rs") && filename != "mod.rs" {
                Some(filename.trim_end_matches(".rs").to_string())
            } else {
                None
            }
        })
        .collect();

    let mut mod_content = String::from(
        "// Copyright 2025 Bloxide, all rights reserved\n// Auto-generated module.\n// Files in this directory are generated by bloxide-codegen.\n",
    );
    for mod_name in mod_names {
        // topology module exports a #[macro_export] handler_table macro
        if mod_name == "topology" {
            mod_content.push_str("#[macro_use]\n");
        }
        mod_content.push_str(&format!("pub mod {};\n", mod_name));
        mod_content.push_str(&format!(
            "#[allow(unused_imports)]\npub use {}::*;\n",
            mod_name
        ));
    }
    // Only write when the module list changed — unconditional writes churn
    // mtime and make every `generate` look non-idempotent.
    let needs_write = if generated_mod.exists() {
        std::fs::read_to_string(&generated_mod)? != mod_content
    } else {
        true
    };
    if needs_write {
        std::fs::write(&generated_mod, mod_content)?;
    }

    Ok(())
}
