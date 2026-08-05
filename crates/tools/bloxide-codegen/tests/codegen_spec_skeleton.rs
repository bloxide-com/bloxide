// Copyright 2025 Bloxide, all rights reserved

use bloxide_codegen::generate_all;
use bloxide_codegen::schema::BloxConfig;

#[test]
fn generate_spec_skeleton_counter() {
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

[[context.fields]]
name = "count"
type = "u32"

[topology]

[[topology.states]]
name = "Ready"
initial = true

[[topology.states]]
name = "Done"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "counter-blox").expect("generate failed");

    let spec_file = files
        .iter()
        .find(|(n, _)| n == "spec_skeleton.rs")
        .expect("spec_skeleton.rs missing");
    let content = &spec_file.1;

    assert!(content.contains("pub struct CounterSpec"));
    assert!(content.contains("MachineSpec"));
    assert!(content.contains("for CounterSpec"));
    assert!(content.contains("type State = CounterState"));
    assert!(content.contains("type Event = CounterEvent"));
    assert!(content.contains("counter_messages::CounterMsg"));
    assert!(content.contains("counter_state_handler_table"));
    assert!(content.contains("CounterState::Ready"));
    assert!(content.contains("fn on_init_entry"));
}

#[test]
fn generate_spec_skeleton_ping() {
    let toml = r#"
[actor]
name = "Ping"

[event]
name = "PingEvent"

[[event.mailboxes]]
variant = "Msg"
message = "PingPongMsg"
message_path = "ping_pong_messages::PingPongMsg"

[context]
name = "PingCtx"
generics = "<R: BloxRuntime>"

[[context.uses]]
field = "peer_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

[[context.fields]]
name = "current_timer"
type = "Option<TimerId>"

[[context.fields]]
name = "round"
type = "u32"

[topology]

[[topology.states]]
name = "Operating"
composite = true

[[topology.states]]
name = "Active"
parent = "Operating"
initial = true

[[topology.states]]
name = "Paused"
parent = "Operating"

[[topology.states]]
name = "Error"
error = true
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "ping-blox").expect("generate failed");

    let spec_file = files
        .iter()
        .find(|(n, _)| n == "spec_skeleton.rs")
        .expect("spec_skeleton.rs missing");
    let content = &spec_file.1;

    assert!(content.contains("pub struct PingSpec"));
    assert!(content.contains("MachineSpec"));
    assert!(content.contains("for PingSpec"));
    assert!(content.contains("type State = PingState"));
    assert!(content.contains("PingState::Active"));
    assert!(content.contains("fn is_error"));
    assert!(content.contains("PingState::Error"));
    assert!(content.contains("ping_state_handler_table"));
}

#[test]
fn no_context_no_spec_skeleton() {
    let toml = r#"
[actor]
name = "Minimal"

[topology]

[[topology.states]]
name = "Idle"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "minimal-blox").expect("generate failed");

    assert!(files.iter().all(|(n, _)| n != "ctx.rs"));
    assert!(files.iter().all(|(n, _)| n != "spec_skeleton.rs"));
}

#[test]
fn generate_spec_skeleton_non_delegatable_uses_for_on_init() {
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
on_init = "ctx.worker_refs_mut().clear(); ctx.set_pending(0);"

[[context.fields]]
name = "worker_refs"
type = "Vec<ActorRef<WorkerMsg, R>>"

[[context.fields]]
name = "pending"
type = "u32"

[topology]

[[topology.states]]
name = "Idle"
initial = true

[[topology.states]]
name = "Active"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "pool-blox").expect("generate failed");
    let skeleton_file = files
        .iter()
        .find(|(n, _)| n == "spec_skeleton.rs")
        .expect("spec_skeleton.rs missing");
    let content = &skeleton_file.1;

    // The spec_skeleton should emit on_init_entry with the on_init body.
    // The old non-delegatable (impl_macro) crate imports are no longer emitted
    // by the codegen since accessor traits are gone.
    assert!(content.contains("on_init_entry"));
    assert!(content.contains("ctx.worker_refs_mut().clear();"));
    assert!(content.contains("ctx.set_pending(0);"));
}
