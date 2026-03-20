# Blox Spec: `Counter`

## Purpose

The Counter actor is the simplest possible bloxide actor, designed for teaching the five-layer architecture. It:
- Receives `Tick` messages and increments an internal counter
- Reaches terminal state after a configurable number of ticks
- Demonstrates: flat state topology, behavior trait injection, terminal state detection

## Crate Location

- Blox crate: `crates/bloxes/counter/`
- Messages crate: `crates/messages/counter-messages/`
- Actions crate: `crates/actions/counter-actions/`
- Impl crate: `crates/impl/counter-demo-impl/` (provides `CounterBehavior`)

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Ready : runtime calls machine.start()
    Ready --> Done : CounterMsg::Tick [count >= DONE_AT_COUNT]
```

> `[Init]` is engine-implicit. `Ready` and `Done` are leaf states.

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for `start()`; `on_init_entry` resets count to 0 |
| `Ready` | leaf | Accepting ticks; count < threshold |
| `Done` | leaf, terminal | Terminal state; `is_terminal()` returns `true` |

## Events

| Event | Handled by | Rule pattern | Guard outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| `CounterMsg::Tick` | `Ready` | Action-Then-Guard | `Done` if count >= threshold, else `Stay` | `increment_count` |
| any unhandled | root (no rules) | — | dropped | none |

## Context

```rust
#[derive(BloxCtx)]
pub struct CounterCtx<B: CountsTicks> {
    pub self_id: ActorId,
    
    #[delegates(CountsTicks)]
    pub behavior: B,
}
```

| Field | Type | Annotation | Description |
|-------|------|------------|-------------|
| `self_id` | `ActorId` | Auto-detected `HasSelfId` | Actor identity |
| `behavior` | `B` | `#[delegates(CountsTicks)]` | Injected behavior; stores count |

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
| `[Init]` (engine) | reset count to 0 via `ctx.set_count(0)` | — |
| `Ready` | — | — |
| `Done` | — | — |

## Constants

| Name | Value | Description |
|------|-------|-------------|
| `DONE_AT_COUNT` | 2 | Ticks required to reach Done |

## Acceptance Criteria

- [ ] `machine.start()` exits Init and enters `Ready`
- [ ] `CounterMsg::Tick` in `Ready` with `count < DONE_AT_COUNT` stays in `Ready`
- [ ] `CounterMsg::Tick` in `Ready` with `count >= DONE_AT_COUNT` transitions to `Done`
- [ ] `is_terminal(&CounterState::Done)` returns `true`
- [ ] `machine.reset()` from `Done` exits states, calls `on_init_entry`, count reset to 0

## Acceptance Criteria → Test Mapping

| Acceptance Criterion | Test Function |
|---|---|
| `machine.start()` exits Init → Ready | `test_start_enters_ready()` |
| Tick stays in Ready when count < threshold | `test_tick_in_ready_stays()` |
| Tick transitions to Done at threshold | `test_tick_reaches_done()` |
| `is_terminal(Done)` returns true | `test_done_is_terminal()` |

## Action Crate Dependencies

| Trait | From crate | Implemented by |
|-------|-----------|----------------|
| `CountsTicks` | `counter-actions` | `CounterBehavior` in impl crate |

## Related Docs

- See `spec/architecture/12-action-crate-pattern.md` for the five-layer model
- See `tokio-minimal-demo.rs` for wiring example
