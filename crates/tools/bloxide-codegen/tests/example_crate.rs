// Copyright 2025 Bloxide, all rights reserved
//! Tests for pure-TOML example crate materialization (example_crate module):
//! crate-name derivation, Cargo.toml emission (path/version/generated-blox/
//! inline deps + derived dev-dependencies), lib.rs / build.rs emission, the
//! test-source crate scan, and an end-to-end sync into a scratch dir.

use bloxide_codegen::example_crate;
use bloxide_codegen::schema::SystemConfig;
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

fn system_config(example: &str) -> SystemConfig {
    let path = workspace_root()
        .join("examples")
        .join(example)
        .join("system.toml");
    let content = std::fs::read_to_string(&path).expect("read system.toml");
    toml::from_str(&content).expect("parse system.toml")
}

fn manifest(example: &str, crate_name: &str) -> String {
    let root = workspace_root();
    let config = system_config(example);
    let discovered = util::discover_bloxes(&root).expect("discover bloxes");
    let blox_configs = discovered
        .iter()
        .map(|(k, v)| (k.clone(), v.config.clone()))
        .collect();
    let tests_dir = root.join("examples").join(example).join("tests");
    let dev_crates = example_crate::test_imported_crates(&tests_dir);
    let example_dir = root
        .join("target/bloxide-generated/examples")
        .join(crate_name);
    example_crate::example_manifest(
        &config,
        crate_name,
        &discovered,
        &blox_configs,
        &root,
        &example_dir,
        &dev_crates,
    )
    .expect("manifest")
}

#[test]
fn crate_name_from_system_name() {
    let root = workspace_root();
    let config = system_config("tokio-demo");
    let name =
        example_crate::example_crate_name(&config, &root.join("examples/tokio-demo/system.toml"));
    assert_eq!(name, "tokio-demo");

    // Fallback: directory name when [system] name is absent.
    let config: SystemConfig = toml::from_str("[system]\nruntime = \"tokio\"\n").unwrap();
    let name =
        example_crate::example_crate_name(&config, Path::new("/x/examples/foo-bar/system.toml"));
    assert_eq!(name, "foo-bar");
}

#[test]
fn tokio_demo_manifest_emission() {
    let out = manifest("tokio-demo", "tokio-demo");

    assert!(out.contains("# Copyright 2025 Bloxide, all rights reserved"));
    assert!(out.contains("name = \"tokio-demo\""));
    assert!(out.contains("publish = false"));
    // Explicit workspace package values (the generated workspace has no
    // [workspace.package] to inherit from).
    assert!(out.contains("version = \"0.0.3\""));
    assert!(out.contains("edition = \"2021\""));
    assert!(out.contains("license = \"MIT\""));

    // Repo crates: relative path deps back into the repo.
    assert!(out.contains(
        "bloxide-core = { path = \"../../../../crates/bloxide-core\", features = [\"std\"] }"
    ));
    assert!(out.contains("bloxide-tokio = { path = \"../../../../runtimes/bloxide-tokio\" }"));
    assert!(out.contains("bloxide-supervisor = { path = \"../../../../crates/bloxide-supervisor\", features = [\"std\"] }"));
    assert!(out.contains(
        "bloxide-timer = { path = \"../../../../crates/bloxide-timer\", features = [\"std\"] }"
    ));
    assert!(out.contains(
        "ping-pong-messages = { path = \"../../../../crates/messages/ping-pong-messages\" }"
    ));
    assert!(out.contains(
        "blox-ctx-ping-pong = { path = \"../../../../crates/context/blox-ctx-ping-pong\" }"
    ));

    // Pure-TOML bloxes: path deps into the generated workspace's crates/.
    assert!(out.contains("ping-blox = { path = \"../../crates/ping-blox\" }"));
    assert!(out.contains("pong-blox = { path = \"../../crates/pong-blox\" }"));

    // External deps: explicit versions copied from the root
    // [workspace.dependencies] (no `{ workspace = true }` anywhere).
    assert!(out.contains("tokio = { version = \"1\", features = [\"full\"] }"));
    assert!(out.contains("tracing = \"0.1\""));
    assert!(out.contains("tracing-log = \"0.2\""));
    assert!(out.contains("tracing-subscriber = { version = \"0.3\", features = [\"env-filter\"] }"));
    assert!(
        !out.contains("workspace = true"),
        "no workspace deps may remain in a generated example manifest:\n{}",
        out
    );

    // No tests/ in the source → no [dev-dependencies] section.
    assert!(!out.contains("[dev-dependencies]"));

    // The codegen itself is a build dependency (build.rs re-sync).
    assert!(out.contains("[build-dependencies]"));
    assert!(
        out.contains("bloxide-codegen = { path = \"../../../../crates/tools/bloxide-codegen\" }")
    );

    let _: toml::Value = toml::from_str(&out).expect("manifest parses as TOML");
}

#[test]
fn pool_demo_manifest_infers_dynamic_feature_and_dev_deps() {
    let out = manifest("tokio-pool-demo", "tokio-pool-demo");

    // Factory injection (source = "factory") infers the `dynamic` feature.
    assert!(
        out.contains("pool-blox = { path = \"../../crates/pool-blox\", features = [\"dynamic\"] }"),
        "pool-blox should carry the inferred dynamic feature:\n{}",
        out
    );
    assert!(
        out.contains("tokio-pool-demo-impl = { path = \"../../../../crates/impl/tokio-pool-demo-impl\", features = [\"dynamic\"] }"),
        "impl crate should carry the inferred dynamic feature:\n{}",
        out
    );
    // The spawn factory composition needs bloxide-spawn.
    assert!(out.contains(
        "bloxide-spawn = { path = \"../../../../crates/bloxide-spawn\", features = [\"std\"] }"
    ));

    // Dev-dependencies derived from the test sources: every crate the tests
    // import directly, except the example crate itself.
    assert!(out.contains("[dev-dependencies]"));
    let dev_section = out.split("[dev-dependencies]").nth(1).unwrap();
    let dev_section = dev_section.split("[build-dependencies]").next().unwrap();
    for dep in [
        "pool-blox",
        "blox-ctx-pool-ref",
        "bloxide-core",
        "bloxide-peers",
        "bloxide-spawn",
        "bloxide-supervisor",
        "bloxide-tokio",
        "pool-messages",
        "tokio",
        "tokio-pool-demo-impl",
    ] {
        assert!(
            dev_section.contains(dep),
            "dev-dependencies missing {}:\n{}",
            dep,
            dev_section
        );
    }
    assert!(
        !dev_section.contains("tokio-pool-demo ="),
        "the example crate itself must not be a dev-dependency:\n{}",
        dev_section
    );

    let _: toml::Value = toml::from_str(&out).expect("manifest parses as TOML");
}

#[test]
fn embassy_demo_manifest_uses_inline_embassy_versions() {
    let out = manifest("embassy-demo", "embassy-demo");

    assert!(out.contains("bloxide-embassy = { path = \"../../../../runtimes/bloxide-embassy\", features = [\"std\"] }"));
    // Embassy crates keep their inline-version behavior.
    assert!(out.contains("embassy-executor = { version = \"0.9\""));
    assert!(out.contains("embassy-sync = { version = \"0.7\""));
    assert!(out.contains("embassy-time = { version = \"0.5\""));
    assert!(out.contains("critical-section = { version = \"1.2\""));
    assert!(!out.contains("bloxide-tokio"));

    let _: toml::Value = toml::from_str(&out).expect("manifest parses as TOML");
}

#[test]
fn test_source_crate_scan() {
    let tests_dir = workspace_root().join("examples/tokio-pool-demo/tests");
    let crates = example_crate::test_imported_crates(&tests_dir);
    // All directly imported crates are found (the scan is a superset: it
    // also yields lowercase module segments like `lifecycle`, which the
    // manifest builder drops because they resolve to no known crate).
    for expected in [
        "blox_ctx_pool_ref",
        "bloxide_child_management",
        "bloxide_core",
        "bloxide_peers",
        "bloxide_spawn",
        "bloxide_supervisor",
        "bloxide_tokio",
        "pool_blox",
        "pool_messages",
        "tokio",
        "tokio_pool_demo",
        "tokio_pool_demo_impl",
    ] {
        assert!(
            crates.contains(expected),
            "scan missing {}: {:?}",
            expected,
            crates
        );
    }

    // A missing tests directory yields no crates.
    let empty = example_crate::test_imported_crates(Path::new("/nonexistent/tests"));
    assert!(empty.is_empty());
}

#[test]
fn build_rs_emission() {
    let root = workspace_root();
    let system_toml = root.join("examples/tokio-demo/system.toml");
    let example_dir = root.join("target/bloxide-generated/examples/tokio-demo");
    let build_rs =
        example_crate::build_rs_source(&system_toml, &root, &example_dir).expect("build.rs");

    assert!(build_rs.contains("cargo:rerun-if-changed"));
    assert!(build_rs.contains("../../../../examples/tokio-demo/system.toml"));
    assert!(build_rs.contains("../../../../examples/tokio-demo/tests"));
    assert!(build_rs.contains("bloxide_codegen::example_crate::sync_from_source"));

    syn::parse_str::<syn::File>(&build_rs).expect("build.rs parses");
}

#[test]
fn lib_rs_emission() {
    let lib = example_crate::lib_rs_source("tokio-pool-demo");
    assert!(lib.contains("// Copyright 2025 Bloxide, all rights reserved"));
    assert!(lib.contains("pub mod generated;"));
    syn::parse_str::<syn::File>(&lib).expect("lib.rs parses");
}

#[test]
fn sync_example_crate_end_to_end() {
    let root = workspace_root();
    let scratch = root
        .join("target/bloxide-codegen-tests")
        .join(format!("sync-example-{}", std::process::id()));
    let example_dir = scratch.join("examples/tokio-pool-demo");
    let system_toml = root.join("examples/tokio-pool-demo/system.toml");

    let crate_name =
        example_crate::sync_example_crate(&system_toml, &root, &example_dir).expect("sync");
    assert_eq!(crate_name, "tokio-pool-demo");

    // Full crate layout materialized.
    for rel in [
        "Cargo.toml",
        "build.rs",
        "src/main.rs",
        "src/lib.rs",
        "src/generated/mod.rs",
        "src/generated/pool_spec_skeleton.rs",
        "src/generated/worker_spec_skeleton.rs",
        "src/generated/bloxide_supervisor_spec_skeleton.rs",
        "tests/pool_lifecycle.rs",
        "tests/pool_behavior.rs",
    ] {
        assert!(
            example_dir.join(rel).exists(),
            "expected {} to exist",
            example_dir.join(rel).display()
        );
    }

    // main.rs selects the Tokio runtime and references the concrete specs.
    let main_rs = std::fs::read_to_string(example_dir.join("src/main.rs")).unwrap();
    assert!(main_rs.contains("#[tokio::main]"));
    assert!(main_rs.contains("worker_spec_skeleton::WorkerSpec"));

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
    let before = snapshot(&example_dir);
    example_crate::sync_example_crate(&system_toml, &root, &example_dir).expect("re-sync");
    let after = snapshot(&example_dir);
    assert_eq!(before, after, "re-sync must be a no-op diff-wise");

    std::fs::remove_dir_all(&scratch).ok();
}
