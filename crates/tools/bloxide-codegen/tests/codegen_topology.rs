// Copyright 2025 Bloxide, all rights reserved

use bloxide_codegen::schema::BloxConfig;
use bloxide_codegen::{generate_all, generate_from_toml};

#[test]
fn generate_counter_topology() {
    let toml = r#"
[actor]
name = "Counter"

[event]
name = "CounterEvent"

[[event.mailboxes]]
variant = "Msg"
message = "CounterMsg"
message_path = "counter_messages::CounterMsg"

[context]
name = "CounterCtx"
generics = "<R: BloxRuntime>"

[topology]

[[topology.states]]
name = "Ready"

[[topology.states]]
name = "Done"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "counter-blox").expect("generate failed");

    let topo_file = files
        .iter()
        .find(|(n, _)| n == "topology.rs")
        .expect("topology.rs missing");
    let content = &topo_file.1;

    // Enum
    assert!(content.contains("pub enum CounterState {"));
    assert!(content.contains("Ready = 0u8,"));
    assert!(content.contains("Done = 1u8,"));
    assert!(content.contains("#[repr(u8)]"));

    // StateTopology impl
    assert!(content.contains("impl ::bloxide_core::topology::StateTopology for CounterState"));
    assert!(content.contains("const STATE_COUNT: usize = 2usize;"));

    // parent() — all top-level => None
    assert!(content.contains("fn parent(self) -> ::core::option::Option<Self>"));

    // is_leaf() — no composites => all true
    assert!(content.contains("fn is_leaf(self) -> bool"));

    // path() — empty static arrays for top-level states
    assert!(content.contains("fn path(self) -> &'static [Self]"));

    // as_index()
    assert!(content.contains("fn as_index(self) -> usize"));

    // Handler table macro
    assert!(content.contains("macro_rules! counter_state_handler_table {"));
    assert!(content.contains("READY_FNS"));
    assert!(content.contains("DONE_FNS"));
}

#[test]
fn generate_composite_topology() {
    let toml = r#"
[actor]
name = "Ping"

[topology]

[[topology.states]]
name = "Operating"
composite = true

[[topology.states]]
name = "Active"
parent = "Operating"

[[topology.states]]
name = "Paused"
parent = "Operating"

[[topology.states]]
name = "Done"

[[topology.states]]
name = "Error"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "ping-blox").expect("generate failed");

    let topo_file = files
        .iter()
        .find(|(n, _)| n == "topology.rs")
        .expect("topology.rs missing");
    let content = &topo_file.1;

    // Enum
    assert!(content.contains("pub enum PingState {"));

    // Operating is composite => not leaf
    assert!(content.contains("Self::Operating => false"));
    // Active is leaf
    assert!(content.contains("Self::Active => true"));
    // Paused is leaf
    assert!(content.contains("Self::Paused => true"));

    // Parent relationships
    assert!(content.contains("Self::Active => ::core::option::Option::Some(Self::Operating)"));
    assert!(content.contains("Self::Paused => ::core::option::Option::Some(Self::Operating)"));

    // Path static arrays for children include parent
    assert!(content.contains("static __PATH_ACTIVE"));
    assert!(content.contains("static __PATH_PAUSED"));
}

#[test]
fn generate_from_toml_roundtrip() {
    let toml = r#"
[actor]
name = "TestActor"

[[messages]]
name = "TestMsg"
visibility = "pub"

[[messages.variants]]
name = "Hello"
[[messages.variants.fields]]
name = "count"
ty = "u32"

[event]
name = "TestEvent"

[[event.mailboxes]]
variant = "Msg"
message = "TestMsg"
message_path = "test_messages::TestMsg"

[topology]

[[topology.states]]
name = "StateA"
"#;

    let tmp_dir = std::env::temp_dir().join("bloxide_codegen_test");
    std::fs::create_dir_all(&tmp_dir).expect("create dir");
    let toml_path = tmp_dir.join("blox.toml");
    std::fs::write(&toml_path, toml).expect("write toml");

    let files = generate_from_toml(&toml_path).expect("generate from toml failed");

    // cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);

    let names: Vec<_> = files.iter().map(|(n, _)| n.clone()).collect();
    assert!(names.contains(&"messages_testmsg.rs".to_string()));
    assert!(names.contains(&"events.rs".to_string()));
    assert!(names.contains(&"topology.rs".to_string()));
}

#[test]
fn cycle_detection() {
    let toml = r#"
[topology]

[[topology.states]]
name = "A"
parent = "B"

[[topology.states]]
name = "B"
parent = "A"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let result = generate_all(&config, "test");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("cycle detected"));
}

#[test]
fn unknown_parent() {
    let toml = r#"
[topology]

[[topology.states]]
name = "A"
parent = "NonExistent"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let result = generate_all(&config, "test");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("unknown parent"));
}

#[test]
fn state_flags_in_topology() {
    let toml = r#"
[actor]
name = "Test"

[topology]

[[topology.states]]
name = "Ready"
initial = true

[[topology.states]]
name = "Done"

[[topology.states]]
name = "Error"
error = true
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let topo = config.topology.unwrap();
    assert_eq!(topo.states[0].initial, Some(true));
}
