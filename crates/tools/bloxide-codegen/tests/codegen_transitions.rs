// Copyright 2025 Bloxide, all rights reserved

use bloxide_codegen::generate_all;
use bloxide_codegen::schema::BloxConfig;

#[test]
fn declarative_transitions_simple_stay() {
    let toml = r#"
[actor]
name = "Pong"

[event]
name = "PongEvent"

[[event.mailboxes]]
variant = "Msg"
message = "PingPongMsg"
message_path = "ping_pong_messages::PingPongMsg"

[context]
name = "PongCtx"
generics = "<R: BloxRuntime>"

[topology]

[[topology.states]]
name = "Ready"
initial = true

[[topology.transitions]]
state = "Ready"
event = "PingPongMsg::Ping(_)"
target = "stay"
actions = ["reply_pong_action"]
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "pong-blox").expect("generate failed");

    let spec_file = files
        .iter()
        .find(|(n, _)| n == "spec_skeleton.rs")
        .expect("spec_skeleton.rs missing");
    let content = &spec_file.1;

    // StateFns constant generated
    assert!(content.contains("READY_FNS"));
    assert!(content.contains("StateFns"));
    // Raw StateRule struct literal emitted by codegen
    assert!(content.contains("StateRule"));
    // Event pattern in matches closure
    assert!(content.contains("PingPongMsg::Ping"));
    // Decision::Stay
    assert!(content.contains("Decision::Stay"));
    // Action function
    assert!(content.contains("reply_pong_action"));
    // Handler table array
    assert!(content.contains("HANDLER_TABLE"));
}

#[test]
fn declarative_transitions_with_guard() {
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
initial = true

[[topology.states]]
name = "Done"

[[topology.transitions]]
state = "Ready"
event = "CounterMsg::Tick(_)"
target = "stay"
actions = ["count_tick"]

[[topology.transitions]]
state = "Ready"
event = "CounterMsg::Tick(_)"
target = "Done"
actions = ["count_tick"]

[[topology.transitions.guards]]
condition = "ctx.count() >= 2"
target = "Done"

[[topology.transitions.guards]]
condition = "ctx.count() < 2"
target = "stay"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "counter-blox").expect("generate failed");

    let spec_file = files
        .iter()
        .find(|(n, _)| n == "spec_skeleton.rs")
        .expect("spec_skeleton.rs missing");
    let content = &spec_file.1;

    // Decision chain generated as if/else-if/else
    assert!(content.contains("ctx.count() >= 2"));
    assert!(content.contains("ctx.count() < 2"));
    // State targets in Decision::Transition
    assert!(content.contains("CounterState::Done"));
    // Decision::Stay for fallback
    assert!(content.contains("Decision::Stay"));
    // Raw StateRule struct literal emitted by codegen
    assert!(content.contains("StateRule"));
}

#[test]
fn declarative_entry_exit() {
    let toml = r#"
[actor]
name = "Worker"

[event]
name = "WorkerEvent"

[[event.mailboxes]]
variant = "Msg"
message = "WorkerMsg"
message_path = "pool_messages::WorkerMsg"

[context]
name = "WorkerCtx"
generics = "<R: BloxRuntime>"

[topology]

[[topology.states]]
name = "Waiting"
initial = true

[[topology.states]]
name = "Done"

[[topology.entry]]
state = "Waiting"
actions = ["log_waiting"]

[[topology.exit]]
state = "Waiting"
actions = ["cleanup"]

[[topology.transitions]]
state = "Waiting"
event = "WorkerMsg::DoWork(_)"
target = "Done"
actions = ["process_work"]
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "worker-blox").expect("generate failed");

    let spec_file = files
        .iter()
        .find(|(n, _)| n == "spec_skeleton.rs")
        .expect("spec_skeleton.rs missing");
    let content = &spec_file.1;

    // Entry/exit actions in StateFns
    assert!(content.contains("log_waiting"));
    assert!(content.contains("cleanup"));
    assert!(content.contains("on_entry"));
    assert!(content.contains("on_exit"));
}

#[test]
fn declarative_transition_with_transition_target() {
    let toml = r#"
[actor]
name = "Pool"

[event]
name = "PoolEvent"

[[event.mailboxes]]
variant = "Msg"
message = "PoolMsg"
message_path = "pool_messages::PoolMsg"

[context]
name = "PoolCtx"
generics = "<R: BloxRuntime>"

[topology]

[[topology.states]]
name = "Idle"
initial = true

[[topology.states]]
name = "Active"

[[topology.states]]
name = "AllDone"

[[topology.transitions]]
state = "Idle"
event = "PoolMsg::SpawnWorker(_)"
target = "Active"
actions = ["handle_spawn_worker"]

[[topology.transitions]]
state = "Active"
event = "PoolMsg::WorkDone(_)"
target = "stay"
actions = ["handle_work_done"]

[[topology.transitions.guards]]
condition = "ctx.pending == 0"
target = "AllDone"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "pool-blox").expect("generate failed");

    let spec_file = files
        .iter()
        .find(|(n, _)| n == "spec_skeleton.rs")
        .expect("spec_skeleton.rs missing");
    let content = &spec_file.1;

    // transition targets (not stay/reset/fail)
    // Decision::Transition with LeafState::new
    assert!(content.contains("Decision::Transition"));
    assert!(content.contains("PoolState::Active"));
    assert!(content.contains("PoolState::AllDone"));
}

#[test]
fn declarative_transition_unknown_state() {
    let toml = r#"
[actor]
name = "Test"

[topology]

[[topology.states]]
name = "Ready"
initial = true

[[topology.transitions]]
state = "Ready"
event = "Msg::A(_)"
target = "NonExistentState"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let result = generate_all(&config, "test-blox");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("unknown target state"));
}

#[test]
fn declarative_transition_unknown_handling_state() {
    let toml = r#"
[actor]
name = "Test"

[topology]

[[topology.states]]
name = "Ready"
initial = true

[[topology.transitions]]
state = "NonExistent"
event = "Msg::A(_)"
target = "stay"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let result = generate_all(&config, "test-blox");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("unknown state"));
}

#[test]
fn declarative_transitions_empty_for_state() {
    let toml = r#"
[actor]
name = "Test"

[event]
name = "TestEvent"

[[event.mailboxes]]
variant = "Msg"
message = "TestMsg"
message_path = "test_messages::TestMsg"

[context]
name = "TestCtx"
generics = "<R: BloxRuntime>"

[topology]

[[topology.states]]
name = "Ready"
initial = true

[[topology.states]]
name = "Done"

[[topology.transitions]]
state = "Ready"
event = "Msg::Go(_)"
target = "Done"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "test-blox").expect("generate failed");

    let spec_file = files
        .iter()
        .find(|(n, _)| n == "spec_skeleton.rs")
        .expect("spec_skeleton.rs missing");
    let content = &spec_file.1;

    // Done state should have empty transitions: &[]
    assert!(content.contains("DONE_FNS"));
    // The Done StateFns should have empty transitions
    // (both states get StateFns, but Done has no transition entries)
    assert!(content.contains("transitions: &[]"));
}

#[test]
fn declarative_reset_fail_targets() {
    let toml = r#"
[actor]
name = "Test"

[event]
name = "TestEvent"

[[event.mailboxes]]
variant = "Msg"
message = "TestMsg"
message_path = "test_messages::TestMsg"

[context]
name = "TestCtx"
generics = "<R: BloxRuntime>"

[topology]

[[topology.states]]
name = "Ready"
initial = true

[[topology.states]]
name = "Error"
error = true

[[topology.transitions]]
state = "Ready"
event = "Msg::Reset(_)"
target = "reset"

[[topology.transitions]]
state = "Ready"
event = "Msg::Fail(_)"
target = "fail"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "test-blox").expect("generate failed");

    let spec_file = files
        .iter()
        .find(|(n, _)| n == "spec_skeleton.rs")
        .expect("spec_skeleton.rs missing");
    let content = &spec_file.1;

    assert!(content.contains("Decision::Reset"));
    assert!(content.contains("Decision::Fail"));
}
