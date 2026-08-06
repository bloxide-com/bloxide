// Copyright 2025 Bloxide, all rights reserved
//! Tests for pure-TOML blox crate materialization (blox_crate module):
//! crate-name derivation, Cargo.toml emission, lib.rs emission, build.rs
//! emission, feature synthesis, and an end-to-end sync into a scratch dir.

use bloxide_codegen::blox_crate;
use bloxide_codegen::schema::BloxConfig;
use bloxide_codegen::util;
use std::path::{Path, PathBuf};

/// Repo workspace root (this test crate lives at crates/tools/bloxide-codegen).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn ping_config() -> BloxConfig {
    let path = workspace_root().join("bloxes/ping/blox.toml");
    let content = std::fs::read_to_string(&path).expect("read ping blox.toml");
    toml::from_str(&content).expect("parse ping blox.toml")
}

#[test]
fn pure_toml_crate_name_from_actor() {
    let config = ping_config();
    assert_eq!(
        util::pure_toml_crate_name(&config).as_deref(),
        Some("ping-blox")
    );

    let config: BloxConfig = toml::from_str("[actor]\nname = \"BhsmTst\"\n").unwrap();
    assert_eq!(
        util::pure_toml_crate_name(&config).as_deref(),
        Some("bhsm-tst-blox")
    );
}

#[test]
fn discovery_classifies_source_kinds() {
    let discovered = util::discover_bloxes(&workspace_root()).expect("discover");
    // ping is a pure-TOML blox under bloxes/.
    assert_eq!(
        discovered.get("ping-blox").map(|b| b.kind),
        Some(util::BloxSourceKind::PureToml)
    );
    // pong is likewise a pure-TOML blox under bloxes/.
    assert_eq!(
        discovered.get("pong-blox").map(|b| b.kind),
        Some(util::BloxSourceKind::PureToml)
    );
    // Stdlib/messages crates with a sibling Cargo.toml stay in-crate.
    assert_eq!(
        discovered.get("bloxide-supervisor").map(|b| b.kind),
        Some(util::BloxSourceKind::InCrate)
    );
    assert_eq!(
        discovered.get("ping-pong-messages").map(|b| b.kind),
        Some(util::BloxSourceKind::InCrate)
    );
}

#[test]
fn ping_manifest_emission() {
    let root = workspace_root();
    let config = ping_config();
    let crate_dir = root.join("target/bloxide-generated/crates/ping-blox");
    let manifest =
        blox_crate::crate_manifest(&config, "ping-blox", &root, &crate_dir).expect("manifest");

    assert!(manifest.contains("# Copyright 2025 Bloxide, all rights reserved"));
    assert!(manifest.contains("name = \"ping-blox\""));
    assert!(manifest.contains("publish = false"));
    assert!(manifest.contains("description = \"Ping actor blox — runtime-agnostic\""));

    // Features come from [package.features] verbatim.
    assert!(manifest.contains("default = [\"std\"]"));
    assert!(manifest.contains("std = [\"bloxide-core/std\", \"bloxide-timer/std\"]"));

    // Dependencies derived from message paths / context imports / action
    // crates, as relative path deps back into the repo.
    assert!(manifest.contains("bloxide-core = { path = \"../../../../crates/bloxide-core\" }"));
    assert!(manifest.contains("bloxide-timer = { path = \"../../../../crates/bloxide-timer\" }"));
    assert!(manifest.contains(
        "ping-pong-messages = { path = \"../../../../crates/messages/ping-pong-messages\" }"
    ));
    assert!(manifest.contains(
        "blox-ctx-ping-pong = { path = \"../../../../crates/context/blox-ctx-ping-pong\" }"
    ));
    assert!(manifest
        .contains("blox-ctx-rounds = { path = \"../../../../crates/context/blox-ctx-rounds\" }"));

    // Dev-dependencies from [package.dev-dependencies].
    assert!(manifest.contains("[dev-dependencies]"));
    assert!(manifest.contains(
        "bloxide-core = { path = \"../../../../crates/bloxide-core\", features = [\"std\"] }"
    ));
    assert!(manifest.contains(
        "bloxide-test-runtime = { path = \"../../../../runtimes/bloxide-test-runtime\" }"
    ));

    // The codegen itself is a build dependency (build.rs re-sync).
    assert!(manifest
        .contains("bloxide-codegen = { path = \"../../../../crates/tools/bloxide-codegen\" }"));

    // Manifest must round-trip through the TOML parser.
    let _: toml::Value = toml::from_str(&manifest).expect("manifest parses as TOML");
}

#[test]
fn lib_rs_emission() {
    let config = ping_config();
    let lib = blox_crate::lib_rs_source(&config, false).expect("lib.rs");

    assert!(lib.contains("#![no_std]"));
    assert!(lib.contains("#[cfg(feature = \"std\")]\nextern crate std;"));
    assert!(!lib.contains("extern crate alloc;"));
    assert!(lib.contains("pub mod generated;"));
    assert!(lib.contains("pub mod prelude {"));
    assert!(lib.contains("pub use crate::{PingCtx, PingEvent, PingSpec, PingState};"));
    assert!(lib.contains("pub use generated::{PingCtx, PingEvent, PingSpec, PingState};"));
    assert!(lib.contains("pub const MAX_ROUNDS: u8 = 5;"));
    assert!(lib.contains("pub const PAUSE_AT_ROUND: u8 = 2;"));

    // syn parses it as valid Rust.
    syn::parse_str::<syn::File>(&lib).expect("lib.rs parses");
}

#[test]
fn build_rs_emission() {
    let root = workspace_root();
    let blox_toml = root.join("bloxes/ping/blox.toml");
    let crate_dir = root.join("target/bloxide-generated/crates/ping-blox");
    let build_rs = blox_crate::build_rs_source(&blox_toml, &root, &crate_dir).expect("build.rs");

    assert!(build_rs.contains("cargo:rerun-if-changed"));
    assert!(build_rs.contains("../../../../bloxes/ping/blox.toml"));
    assert!(build_rs.contains("../../../../bloxes/ping/tests"));
    assert!(build_rs.contains("bloxide_codegen::blox_crate::sync_from_source"));

    syn::parse_str::<syn::File>(&build_rs).expect("build.rs parses");
}

#[test]
fn feature_synthesis_for_undeclared_package() {
    // A blox.toml with no [package] section: features must be synthesized —
    // default/std plus an empty feature per feature gate referenced in the
    // TOML (`dynamic` here, mirroring the original pool fixture).
    let toml_src = r#"
[actor]
name = "Foo"

[context]
name = "FooCtx"
generics = "<R: BloxRuntime>"
feature = "dynamic"
feature_generics = "<R: BloxRuntime>"
"#;
    let config: BloxConfig = toml::from_str(toml_src).expect("parse");
    let root = workspace_root();
    let crate_dir = root.join("target/bloxide-generated/crates/foo-blox");
    let manifest =
        blox_crate::crate_manifest(&config, "foo-blox", &root, &crate_dir).expect("manifest");

    assert!(manifest.contains("default = [\"std\"]"));
    assert!(manifest.contains("std = [\"bloxide-core/std\"]"));
    assert!(manifest.contains("dynamic = []"));
    let _: toml::Value = toml::from_str(&manifest).expect("manifest parses as TOML");
}

#[test]
fn undeclared_referenced_feature_is_a_hard_error() {
    // When [package.features] IS declared, every feature gate referenced in
    // the TOML must be listed — otherwise check-cfg rejects the generated
    // code. Missing entries are a hard error.
    let toml_src = r#"
[actor]
name = "Foo"

[package.features]
default = ["std"]
std = ["bloxide-core/std"]

[context]
name = "FooCtx"
generics = "<R: BloxRuntime>"
feature = "dynamic"
feature_generics = "<R: BloxRuntime>"
"#;
    let config: BloxConfig = toml::from_str(toml_src).expect("parse");
    let root = workspace_root();
    let crate_dir = root.join("target/bloxide-generated/crates/foo-blox");
    let err = blox_crate::crate_manifest(&config, "foo-blox", &root, &crate_dir)
        .expect_err("must fail on undeclared referenced feature");
    assert!(err.to_string().contains("\"dynamic\""));
}

#[test]
fn sync_blox_crate_end_to_end() {
    let root = workspace_root();
    let scratch = root
        .join("target/bloxide-codegen-tests")
        .join(format!("sync-{}", std::process::id()));
    let crate_dir = scratch.join("crates/ping-blox");
    let blox_toml = root.join("bloxes/ping/blox.toml");

    let crate_name = blox_crate::sync_blox_crate(&blox_toml, &root, &crate_dir).expect("sync");
    assert_eq!(crate_name, "ping-blox");

    // Full crate layout materialized.
    for rel in [
        "Cargo.toml",
        "build.rs",
        "src/lib.rs",
        "src/generated/mod.rs",
        "src/generated/ctx.rs",
        "src/generated/events.rs",
        "src/generated/topology.rs",
        "src/generated/spec_skeleton.rs",
        "tests/ping.rs",
    ] {
        assert!(
            crate_dir.join(rel).exists(),
            "expected {} to exist",
            crate_dir.join(rel).display()
        );
    }

    // Idempotent: a second sync must not change any file content.
    let snapshot = |dir: &Path| -> Vec<(PathBuf, Vec<u8>)> {
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            files.push((
                entry.path().strip_prefix(dir).unwrap().to_path_buf(),
                std::fs::read(entry.path()).unwrap(),
            ));
        }
        files.sort();
        files
    };
    let before = snapshot(&crate_dir);
    blox_crate::sync_blox_crate(&blox_toml, &root, &crate_dir).expect("re-sync");
    let after = snapshot(&crate_dir);
    assert_eq!(before, after, "re-sync must be a no-op diff-wise");

    std::fs::remove_dir_all(&scratch).ok();
}
