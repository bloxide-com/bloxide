// Copyright 2025 Bloxide, all rights reserved
//! Build script: generate the tuple mailbox impls (`impl Mailboxes<E> for
//! (S1, ..., SK)`, arity 1..=max_arity) at build time.
//!
//! The impls are produced by `bloxide-codegen` from the `[mailboxes]` section
//! of this crate's `blox.toml` and written to `$OUT_DIR/mailboxes_impls.rs`;
//! `src/lib.rs` pulls them in with `include!(concat!(env!("OUT_DIR"), ...))`.
//!
//! They were previously generated into `src/generated/mailboxes_impls.rs` by
//! `cargo blox generate` and gitignored, which forced a manual codegen step
//! before a fresh checkout could build. Generating into `OUT_DIR` removes
//! that bootstrap requirement; `blox.toml` remains the source of truth for
//! `max_arity`.

use std::path::Path;

fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is always set by cargo");
    let blox_toml = Path::new(&manifest_dir).join("blox.toml");

    // Regenerate only when the codegen input changes.
    println!("cargo:rerun-if-changed={}", blox_toml.display());

    let content = std::fs::read_to_string(&blox_toml)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", blox_toml.display(), e));
    let config: bloxide_codegen::schema::BloxConfig = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", blox_toml.display(), e));

    let mailboxes = config.mailboxes.unwrap_or_else(|| {
        panic!(
            "{} has no [mailboxes] section — it configures the tuple mailbox \
             impls generated at build time",
            blox_toml.display()
        )
    });

    let code = bloxide_codegen::mailboxes::generate(&mailboxes)
        .unwrap_or_else(|e| panic!("mailbox codegen failed for {}: {}", blox_toml.display(), e));

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is always set by cargo");
    let out_path = Path::new(&out_dir).join("mailboxes_impls.rs");
    std::fs::write(&out_path, code)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", out_path.display(), e));
}
