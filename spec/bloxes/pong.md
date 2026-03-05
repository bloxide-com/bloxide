# Blox Spec: `Pong`

## Purpose

The Pong actor responds to every `PingPongMsg::Ping` it receives by sending `PingPongMsg::Pong` back to the Ping actor. It is a simple responder: it does not track round counts or decide when to stop — that logic lives in Ping.

## Crate Location

- Blox crate: `examples/bloxes/pong/`
- Messages crate: `examples/messages/ping-pong-messages/`
- Actions crate: `examples/actions/ping-pong-actions/`

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Ready : runtime calls machine.start()

    Ready --> Ready : PingPongMsg::Ping [NoTransition, sends PingPongMsg::Pong]
```

> `[Init]` is engine-implicit (not in the `PongState` enum). The actor enters Init at construction and waits. The runtime calls `machine.start()` to exit Init and enter `Ready`. The runtime calls `machine.reset()` to return to Init.
> `Ready` is the only user-declared state and is a leaf. `Ready → Ready` is `Stay`, not a self-transition — `on_entry` does not fire.

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for runtime `start()`; `on_init_entry` logs "reset" |
| `Ready` | leaf | Actively responding to pings; stays here indefinitely |

## Events

| Event | Handled by | Reaction | Side effects |
|-------|-----------|----------|--------------|
| `PingPongMsg::Ping(_)` | `Ready` | `Stay` | sends `PingPongMsg::Pong` via `send_pong` action |
| any unhandled | root (no rules) | dropped | none |

Lifecycle control (`start`, `reset`) is handled by the runtime — these do not appear as events.

## Context

`PongCtx` uses `#[derive(BloxCtx)]` to generate accessor trait impls and a constructor.

```rust
#[derive(BloxCtx)]
pub struct PongCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
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
| `[Init]` (engine) | logs "reset" via `blox_log_info!` | — |
| `Ready` | — | — |

The response message is sent inside the transition action `reply_pong_action`, defined as a method on `PongSpec<R>` in the blox crate. It extracts the `Ping` payload and delegates to the `send_pong` generic function from `ping-pong-actions`. The action is referenced from the `transitions!` block in the spec.

## Acceptance Criteria

- [x] `machine.start()` exits Init and enters `Ready`; runtime emits `ChildLifecycleEvent::Started`
- [x] `PingPongMsg::Ping(Ping { round: n })` in `Ready` sends `PingPongMsg::Pong(Pong { round: n })` to Ping and returns `Stay`
- [x] `Ready::on_entry` does NOT fire on `PingPongMsg::Ping` (it is `Stay`, not a self-transition)
- [x] `machine.reset()` returns to Init; `on_init_entry` fires (no-op); runtime emits `ChildLifecycleEvent::Reset`
- [x] Unknown events bubble to root (no root rules) and are silently dropped
- [x] Pong has no round counter — it is stateless with respect to round tracking
- [x] `is_terminal()` always returns `false` for Pong — it has no terminal state
- [x] When `send_pong` fails (peer channel full), machine transitions to `Error`; `is_error(&PongState::Error)` returns `true`

## Implementation Notes

- The round echo (`Pong { round: n }` echoes the same `n`) is intentional: Pong is a mirror.
- `try_send` is used (not `send`) because `on_event` runs synchronously inside dispatch.
- Pong does not know when the exchange ends — it will keep responding to pings indefinitely. When Ping transitions to `Done`, it simply stops sending, and Pong's mailbox goes quiet.
- The blox crate only imports `ping-pong-actions` for the `HasPeerRef` trait and `send_pong` function. Concrete logger types come from the binary, not the blox.
- See `spec/architecture/08-supervision.md` for how the runtime manages lifecycle.
- See `spec/architecture/10-action-crate-pattern.md` for the full five-layer architecture.
