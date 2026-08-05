// Copyright 2025 Bloxide, all rights reserved

use bloxide_codegen::generate_all;
use bloxide_codegen::schema::BloxConfig;

#[test]
fn generate_counter_ctx() {
    let toml = r#"
[actor]
name = "Counter"

[context]
name = "CounterCtx"

[[context.fields]]
name = "count"
type = "u32"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "counter-blox").expect("generate failed");
    let ctx_file = files
        .iter()
        .find(|(n, _)| n == "ctx.rs")
        .expect("ctx.rs missing");
    let content = &ctx_file.1;

    // Plain struct — no #[derive(BloxCtx)], no behavior field, no delegate macros.
    assert!(content.contains("pub struct CounterCtx"));
    assert!(content.contains("pub self_id: ::bloxide_core::ActorId"));
    assert!(content.contains("pub fn new(self_id: ::bloxide_core::ActorId) -> Self"));
}

#[test]
fn generate_ping_ctx() {
    let toml = r#"
[actor]
name = "Ping"

[context]
name = "PingCtx"
generics = "<R: BloxRuntime>"

[[context.uses]]
field = "peer_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

[[context.uses]]
field = "self_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

[[context.uses]]
field = "timer_ref"
field_type = "ActorRef<TimerCommand, R>"
role = "ctor"

[[context.fields]]
name = "current_timer"
type = "Option<TimerId>"

[[context.fields]]
name = "round"
type = "u32"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "ping-blox").expect("generate failed");

    let ctx_file = files
        .iter()
        .find(|(n, _)| n == "ctx.rs")
        .expect("ctx.rs missing");
    let content = &ctx_file.1;

    // Plain struct — no #[derive(BloxCtx)], no delegate macros, no behavior field.
    assert!(content.contains("pub struct PingCtx<R: BloxRuntime>"));
    assert!(content.contains("use ::bloxide_core::{capability::BloxRuntime, messaging::ActorRef}"));
    // Fields from uses (role = "ctor" → constructor params)
    assert!(content.contains("pub peer_ref: ActorRef<PingPongMsg, R>"));
    assert!(content.contains("pub self_ref: ActorRef<PingPongMsg, R>"));
    assert!(content.contains("pub timer_ref: ActorRef<TimerCommand, R>"));
    assert!(content.contains("pub self_id: ::bloxide_core::ActorId"));
    // Constructor takes ctor fields, zero-inits state fields
    assert!(content.contains("pub fn new("));
}

#[test]
fn generate_ctx_generics_passthrough() {
    let toml = r#"
[actor]
name = "Counter"

[context]
name = "CounterCtx"
generics = "<R: BloxRuntime>"

[[context.fields]]
name = "count"
type = "u32"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "counter-blox").expect("generate failed");

    let ctx_file = files
        .iter()
        .find(|(n, _)| n == "ctx.rs")
        .expect("ctx.rs missing");
    let content = &ctx_file.1;

    // The free-form generics string is passed through verbatim.
    assert!(content.contains("pub struct CounterCtx<R: BloxRuntime>"));
    assert!(content.contains("pub self_id: ::bloxide_core::ActorId"));
}

#[test]
fn generate_ctx_uses_single_field_ctor() {
    let toml = r#"
[actor]
name = "Ping"

[context]
name = "PingCtx"
generics = "<R: BloxRuntime>"

[[context.uses]]
field = "peer_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

[[context.uses]]
field = "self_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

[[context.fields]]
name = "round"
type = "u32"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let files = generate_all(&config, "ping-blox").expect("generate failed");
    let ctx_file = files
        .iter()
        .find(|(n, _)| n == "ctx.rs")
        .expect("ctx.rs missing");
    let content = &ctx_file.1;

    // Plain struct — no #[derive(BloxCtx)], no #[provides], no delegate macros.
    assert!(content.contains("pub struct PingCtx<R: BloxRuntime>"));
    assert!(content.contains("pub self_id: ::bloxide_core::ActorId"));

    // Single-field uses with role = "ctor" → constructor params
    assert!(content.contains("pub peer_ref: ActorRef<PingPongMsg, R>"));
    assert!(content.contains("pub self_ref: ActorRef<PingPongMsg, R>"));

    // Framework imports (auto-detected from field types containing ActorRef + BloxRuntime)
    assert!(content.contains("use ::bloxide_core::{capability::BloxRuntime, messaging::ActorRef}"));

    // Constructor takes ctor fields
    assert!(content.contains("pub fn new("));
}

#[test]
fn parse_context_uses_single_field_ctor() {
    let toml = r#"
[actor]
name = "Ping"

[context]
name = "PingCtx"
generics = "<R: BloxRuntime>"

[[context.uses]]
field = "peer_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

[[context.uses]]
field = "self_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"
"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let ctx = config.context.expect("context section missing");
    assert_eq!(ctx.uses.len(), 2);

    let u0 = &ctx.uses[0];
    assert_eq!(u0.field.as_deref(), Some("peer_ref"));
    assert_eq!(u0.field_type.as_deref(), Some("ActorRef<PingPongMsg, R>"));
    assert_eq!(u0.role.as_deref(), Some("ctor"));
    assert!(u0.fields.is_empty());

    let u1 = &ctx.uses[1];
    assert_eq!(u1.field.as_deref(), Some("self_ref"));
}

#[test]
fn parse_context_uses_empty_default() {
    // When [[context.uses]] is absent, uses should default to empty vec.
    let toml = r#"
[actor]
name = "Counter"

[context]
name = "CounterCtx"

"#;

    let config: BloxConfig = toml::from_str(toml).expect("parse failed");
    let ctx = config.context.expect("context section missing");
    assert!(ctx.uses.is_empty());
}

// test_parse_context_field_role removed — [[context.fields]] no longer exists
