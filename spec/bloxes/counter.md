# Blox Spec: `Counter`

## Purpose

The Counter actor is the simplest possible bloxide actor, designed for teaching the four-layer architecture. It:
- Receives `Tick` messages and increments an internal counter
- Self-suspends via `Guard::Stop` after a configurable number of ticks
- Demonstrates: flat state topology, plain context struct, self-suspend via Guard::Stop

## Crate Location

- Blox crate: `crates/bloxes/counter/`
- Messages crate: `crates/messages/counter-messages/`
- Context crate: `crates/context/blox-ctx-ticks/` (provides `increment_count`)
- No impl crate needed — behavior is simple enough for context-crate actions

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Ready : dispatch(Start)
    Ready --> [*] : CounterMsg::Tick [count >= DONE_AT_COUNT] : Guard::Stop
```

> `[Init]` is engine-implicit. `Ready` is a leaf state.

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for `dispatch(Start)`; `on_init_entry` resets count to 0 |
| `Ready` | leaf | Accepting ticks; count < threshold |

## Events

| Event | Handled by | Rule pattern | Guard outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| `CounterMsg::Tick` | `Ready` | Action-Then-Guard | `Stop` if count >= threshold, else `Stay` | `increment_count` |
| any unhandled | root (no rules) | — | dropped | none |

## Context

```rust
pub struct CounterCtx {
    pub self_id: ActorId,
    pub count: u32,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `self_id` | `ActorId` | Actor identity (auto-emitted by codegen) |
| `count` | `u32` | Tick counter (plain state field) |

## Message Contracts

### Receives (`CounterMsg`)

| Variant | Payload | Source |
|---------|---------|--------|
| `CounterMsg::Tick(Tick)` | none | External sender (test or wiring) |

### Sends

None — Counter is a sink actor.

## Entry / Exit Actions

| State | on_entry | on_exit |
|-------|----------|---------|
| `[Init]` (engine) | reset count to 0 via `on_init_entry` (ctx.count = 0) | — |
| `Ready` | — | — |

## Constants

| Name | Value | Description |
|------|-------|-------------|
| `DONE_AT_COUNT` | 2 | Ticks required to trigger Guard::Stop |

## Acceptance Criteria

- [ ] `dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start))` exits Init and enters `Ready`
- [ ] `CounterMsg::Tick` in `Ready` with `count < DONE_AT_COUNT` stays in `Ready`
- [ ] `CounterMsg::Tick` in `Ready` with `count >= DONE_AT_COUNT` triggers `Guard::Stop` (self-suspend to Init)
- [ ] `dispatch(CounterEvent::Lifecycle(LifecycleCommand::Reset))` from any state exits states, enters `initial_state()` (Ready); `on_init_entry` does NOT fire; count reset to 0 via `Ready::on_entry`

## Acceptance Criteria → Test Mapping

| Acceptance Criterion | Test Function |
|---|---|
| `dispatch(LifecycleCommand::Start)` exits Init → Ready | `test_start_enters_ready()` |
| Tick stays in Ready when count < threshold | `test_tick_in_ready_stays()` |
| Tick triggers Guard::Stop at threshold | `test_tick_reaches_stopped()` |


## Context Crate Dependencies

| Action function | From crate | Description |
|-------|-----------|----------------|
| `increment_count` | `blox-ctx-ticks` | Increments the `count` field |

## Related Docs

- See `spec/architecture/12-action-crate-pattern.md` for the four-layer model
- See `tokio-minimal-demo.rs` for wiring example
