# Blox Spec: `Pong`

## Purpose

The Pong actor responds to every `PingPongMsg::Ping` it receives by sending `PingPongMsg::Pong` back to the Ping actor. It is a simple responder: it does not track round counts or decide when to stop — that logic lives in Ping.

## Crate Location

- Blox crate: `crates/bloxes/pong/`
- Messages crate: `crates/messages/ping-pong-messages/`
- Context crate: `crates/blox-ctx-ping-pong/` (provides `send_pong` action function)

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Ready : dispatch(Start)

    Ready --> Ready : PingPongMsg::Ping [NoTransition, sends PingPongMsg::Pong]
```

    > `[Init]` is engine-implicit (not in the `PongState` enum). The actor enters Init at construction and waits. `dispatch(PongEvent::Lifecycle(LifecycleCommand::Start))` exits Init and enters `Ready`. `dispatch(PongEvent::Lifecycle(LifecycleCommand::Reset))` goes **directly** to `initial_state()` (Ready) — it skips Init and `on_init_entry` does NOT fire.
> `Ready` is the only user-declared state and is a leaf. `Ready → Ready` is `Stay`, not a self-transition — `on_entry` does not fire.

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for `dispatch(Start)`; `on_init_entry` is the default no-op (Pong has no state fields) |
| `Ready` | leaf | Actively responding to pings; stays here indefinitely |

## Events

| Event | Handled by | Reaction | Side effects |
|-------|-----------|----------|--------------|
| `PingPongMsg::Ping(_)` | `Ready` | `Stay` | sends `PingPongMsg::Pong` via `send_pong` action |
| any unhandled | root (no rules) | dropped | none |

Lifecycle commands (`Start`, `Reset`, `Stop`, `Ping`) arrive as `PongEvent::Lifecycle(...)` events and are intercepted by the engine at the VirtualRoot level — they are never matched against state transition rules.

## Context

`PongCtx` is a plain struct with plain fields.

```rust
pub struct PongCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
}
```

No `supervisor_ref` — actors don't hold a reference to their supervisor.

## Message Contracts

Both Ping and Pong share the `PingPongMsg` enum from `ping-pong-messages`.

### Receives (`PingPongMsg`)

| Variant | Payload | Sent by |
|---------|---------|---------|
| `PingPongMsg::Ping(Ping { round })` | round number | Ping actor |

### Sends

| Target | Message | When |
|--------|---------|------|
| `peer_ref` | `PingPongMsg::Pong(Pong { round })` | `Ready` transition action `send_pong` when `PingPongMsg::Ping` received |

The runtime notifies the supervisor of lifecycle events (`Started`, `Reset`) automatically — no explicit sends from actor code.

## Entry / Exit Actions

| State | on_entry | on_exit |
|-------|----------|---------|
| `[Init]` (engine) | — (default no-op) | — |
| `Ready` | — | — |

The response message is sent inside the transition action `reply_pong_action`
(fn `send_pong` from `blox-ctx-ping-pong`), declared in `[[context.actions]]` in
`blox.toml` with `event_payload = "ping"` — it extracts the `Ping` payload and
echoes the round back to `peer_ref`. There are no logging actions (invariant #15).

## Acceptance Criteria

Blox-crate unit tests run against the blox-level **stub** spec (per invariant #18);
they verify topology and lifecycle semantics, not action side effects.

- [x] `dispatch(PongEvent::Lifecycle(LifecycleCommand::Start))` exits Init and enters `Ready`
- [x] `PingPongMsg::Ping(Ping { round: n })` in `Ready` returns `Stay` (does not leave `Ready`)
- [x] Multiple pings in sequence all stay in `Ready`
- [x] `Ready::on_entry` does NOT fire on `PingPongMsg::Ping` (it is `Stay`, not a self-transition)
- [x] `dispatch(PongEvent::Lifecycle(LifecycleCommand::Reset))` goes directly to `initial_state()` (Ready); `on_init_entry` does NOT fire
- [x] Unknown events bubble to root (no root rules) and are silently dropped
- [x] Pong has no round counter — it is stateless with respect to round tracking
- [x] Pong has no `Guard::Stop` condition — it responds indefinitely until the supervisor stops it

## Implementation Notes

- The round echo (`Pong { round: n }` echoes the same `n`) is intentional: Pong is a mirror.
- `try_send` is used (not `send`) because `on_event` runs synchronously inside dispatch.
- Pong does not know when the exchange ends — it will keep responding to pings indefinitely. When Ping's guard returns `Guard::Stop`, it self-suspends to `Init` and simply stops sending, and Pong's mailbox goes quiet.
- The blox crate only imports `blox-ctx-ping-pong` for the `send_pong` action function. It does NOT depend on `bloxide-log` (invariant #15).
- See `spec/architecture/08-supervision.md` for how the runtime manages lifecycle.
- See `spec/architecture/12-action-crate-pattern.md` for the full four-layer architecture.

## Acceptance Criteria → Test Mapping

All tests live in `crates/bloxes/pong/src/tests.rs` and use `TestRuntime`:

| Acceptance Criterion | Test Function |
|---|---|
| `dispatch(LifecycleCommand::Start)` exits Init → Ready | `start_enters_ready` |
| Ping in Ready → Stay | `ping_in_ready_stays_in_ready` |
| Repeated pings stay in Ready | `multiple_pings_stay_in_ready` |
| Reset → initial_state() directly | `terminate_resets_to_initial_state` |
