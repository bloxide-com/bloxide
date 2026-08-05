// Copyright 2025 Bloxide, all rights reserved

use bloxide_codegen::schema::SystemConfig;

// ---------------------------------------------------------------------------
// system.toml — wiring manifest schema tests
// ---------------------------------------------------------------------------

#[test]
fn parse_system_toml_minimal() {
    // Minimal system.toml — just [system] with a runtime, no actors or supervision.
    let toml = r#"
[system]
runtime = "tokio"
"#;

    let config: SystemConfig = toml::from_str(toml).expect("parse failed");
    assert_eq!(config.system.runtime, "tokio");
    assert!(config.actors.is_empty());
    assert!(config.supervision.is_empty());
}

#[test]
fn parse_system_toml_ping_pong() {
    // Full ping-pong wiring manifest matching spec 17.
    let toml = r#"
[system]
runtime = "tokio"

[[actors]]
name = "timer"
blox = "bloxide-timer"

[[actors]]
name = "ping"
blox = "ping-blox"

  [actors.inject]
  self_ref = { source = "self" }
  peer_ref = { source = "actor", actor = "pong" }
  timer_ref = { source = "actor", actor = "timer" }

[[actors]]
name = "pong"
blox = "pong-blox"

  [actors.inject]
  self_ref = { source = "self" }
  peer_ref = { source = "actor", actor = "ping" }

[[supervision]]
supervisor = "bloxide-supervisor"
strategy = "when_any_done"
children = ["ping", "pong"]

  [supervision.policies]
  ping = { restart = { max = 1 } }
  pong = { stop = true }
"#;

    let config: SystemConfig = toml::from_str(toml).expect("parse failed");
    assert_eq!(config.system.runtime, "tokio");
    assert_eq!(config.actors.len(), 3);
    assert_eq!(config.supervision.len(), 1);

    // Timer actor — no inject
    let timer = &config.actors[0];
    assert_eq!(timer.name, "timer");
    assert_eq!(timer.blox, "bloxide-timer");
    assert!(timer.inject.is_empty());

    // Ping actor — has 3 inject entries
    let ping = &config.actors[1];
    assert_eq!(ping.name, "ping");
    assert_eq!(ping.blox, "ping-blox");
    assert_eq!(ping.inject.len(), 3);

    let self_ref = ping.inject.get("self_ref").expect("self_ref missing");
    assert_eq!(self_ref.source, "self");
    assert!(self_ref.actor.is_none());

    let peer_ref = ping.inject.get("peer_ref").expect("peer_ref missing");
    assert_eq!(peer_ref.source, "actor");
    assert_eq!(peer_ref.actor.as_deref(), Some("pong"));

    let timer_ref = ping.inject.get("timer_ref").expect("timer_ref missing");
    assert_eq!(timer_ref.source, "actor");
    assert_eq!(timer_ref.actor.as_deref(), Some("timer"));

    // Pong actor — 2 inject entries
    let pong = &config.actors[2];
    assert_eq!(pong.name, "pong");
    assert_eq!(pong.inject.len(), 2);
    let pong_peer = pong.inject.get("peer_ref").expect("pong peer_ref missing");
    assert_eq!(pong_peer.actor.as_deref(), Some("ping"));

    // Supervision
    let sup = &config.supervision[0];
    assert_eq!(sup.supervisor, "bloxide-supervisor");
    assert_eq!(sup.strategy, "when_any_done");
    assert_eq!(sup.children, vec!["ping", "pong"]);
    assert_eq!(sup.policies.len(), 2);

    let ping_policy = sup.policies.get("ping").expect("ping policy missing");
    let restart = ping_policy.restart.as_ref().expect("restart missing");
    assert_eq!(restart.max, 1);
    assert!(ping_policy.stop.is_none());

    let pong_policy = sup.policies.get("pong").expect("pong policy missing");
    assert_eq!(pong_policy.stop, Some(true));
    assert!(pong_policy.restart.is_none());
}

#[test]
fn parse_system_toml_multi_mailbox_actor() {
    // Worker actor with a secondary-channel inject (multi-mailbox actors use
    // `source = "self_secondary", index = N`).
    let toml = r#"
[system]
runtime = "tokio"

[[actors]]
name = "worker-1"
blox = "worker-blox"

  [actors.inject]
  self_ref = { source = "self" }
  ctrl_ref = { source = "self_secondary", index = 1 }
  pool_ref = { source = "actor", actor = "pool" }
"#;

    let config: SystemConfig = toml::from_str(toml).expect("parse failed");
    let worker = &config.actors[0];
    assert_eq!(worker.name, "worker-1");

    let self_ref = worker.inject.get("self_ref").expect("self_ref missing");
    assert_eq!(self_ref.source, "self");
    assert!(self_ref.index.is_none());

    let ctrl_ref = worker.inject.get("ctrl_ref").expect("ctrl_ref missing");
    assert_eq!(ctrl_ref.source, "self_secondary");
    assert_eq!(ctrl_ref.index, Some(1));

    let pool_ref = worker.inject.get("pool_ref").expect("pool_ref missing");
    assert_eq!(pool_ref.source, "actor");
    assert_eq!(pool_ref.actor.as_deref(), Some("pool"));
}

#[test]
fn parse_system_toml_embassy_runtime() {
    // Runtime selection: embassy.
    let toml = r#"
[system]
runtime = "embassy"

[[actors]]
name = "ping"
blox = "ping-blox"
"#;

    let config: SystemConfig = toml::from_str(toml).expect("parse failed");
    assert_eq!(config.system.runtime, "embassy");
    assert_eq!(config.actors.len(), 1);
}

#[test]
fn parse_system_toml_test_runtime() {
    // Runtime selection: test.
    let toml = r#"
[system]
runtime = "test"
"#;

    let config: SystemConfig = toml::from_str(toml).expect("parse failed");
    assert_eq!(config.system.runtime, "test");
}

#[test]
fn parse_system_toml_actor_no_behavior() {
    // Actors with no inject entries (e.g. timer service).
    let toml = r#"
[system]
runtime = "tokio"

[[actors]]
name = "timer"
blox = "bloxide-timer"
"#;

    let config: SystemConfig = toml::from_str(toml).expect("parse failed");
    let timer = &config.actors[0];
    assert!(timer.inject.is_empty());
}

#[test]
fn parse_system_toml_missing_system_fails() {
    // [system] table is required — parsing must fail without it.
    let toml = r#"
[[actors]]
name = "ping"
blox = "ping-blox"
"#;

    let result: Result<SystemConfig, _> = toml::from_str(toml);
    assert!(result.is_err());
}

#[test]
fn parse_system_toml_empty_actors_and_supervision() {
    // actors and supervision default to empty vecs when absent.
    let toml = r#"
[system]
runtime = "tokio"
"#;

    let config: SystemConfig = toml::from_str(toml).expect("parse failed");
    assert!(config.actors.is_empty());
    assert!(config.supervision.is_empty());
}

#[test]
fn parse_system_toml_supervision_capacities() {
    let toml = r#"
[system]
runtime = "tokio"

[[supervision]]
supervisor = "bloxide-supervisor"
strategy = "when_all_done"
children = []

  [supervision.capacities]
  notify = 64
  control = 24
"#;

    let config: SystemConfig = toml::from_str(toml).expect("parse failed");
    let caps = &config.supervision[0].capacities;
    assert_eq!(caps.notify, Some(64));
    assert_eq!(caps.control, Some(24));
    assert_eq!(
        caps.lifecycle, None,
        "unset capacity stays None for defaults"
    );

    // Unknown capacity keys are rejected (deny_unknown_fields).
    let bad = toml.replace("control = 24", "bogus = 1");
    assert!(
        toml::from_str::<SystemConfig>(&bad).is_err(),
        "unknown capacities key must be rejected"
    );
}
