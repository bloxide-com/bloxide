# Blox Spec: `Counter`

## Purpose

The Counter actor is the simplest possible bloxide actor, designed for teaching the four-layer architecture. It:
- Receives `Tick` messages and increments an internal counter
- Self-terminates cleanly via `Decision::Done` after `DONE_AT_COUNT` ticks
- Demonstrates: flat state topology, plain context struct, clean self-termination via `Decision::Done`

## Crate Location

- Blox crate: `crates/bloxes/counter/`
- Messages crate: `crates/messages/counter-messages/`
- Context crate: `crates/context/blox-ctx-ticks/` (provides `increment_count`)
- No impl crate needed — behavior is simple enough for context-crate actions

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Ready : dispatch(Start)
    Ready --> [*] : CounterMsg::Tick [count >= DONE_AT_COUNT] : Decision::Done
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
| `CounterMsg::Tick` | `Ready` | Action-Then-Guard | `Decision::Done` if `count >= DONE_AT_COUNT`, else `Decision::Stay` | `increment_count` |
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
| `DONE_AT_COUNT` | 2 | Ticks required to trigger `Decision::Done`; defined in `crates/bloxes/counter/src/lib.rs`, imported into the guard via `spec_imports` |

## Acceptance Criteria

- [x] `dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start))` exits Init and enters `Ready`
- [x] `CounterMsg::Tick` in `Ready` with `count < DONE_AT_COUNT` stays in `Ready`
- [x] `CounterMsg::Tick` in `Ready` with `count >= DONE_AT_COUNT` triggers `Decision::Done` (clean self-termination: exit chain + on_init_entry, then the task ends)
- [x] `dispatch(CounterEvent::Lifecycle(LifecycleCommand::Reset))` from any state goes directly to `initial_state()` (Ready); `on_init_entry` does NOT fire and `Ready` has no `on_entry`, so `count` is **not** reset
- [x] Unhandled events are dropped — `CounterMsg` has only `Tick`, which `Ready` handles, so the only state with no matching rule is Init; a domain event in Init is silently dropped by the engine's Init catch-all (no transition, `count` unchanged)

## Acceptance Criteria → Test Mapping

| Acceptance Criterion | Test Function |
|---|---|
| `dispatch(LifecycleCommand::Start)` exits Init → Ready | `test_start_enters_ready()` |
| Tick stays in Ready when count < threshold | `test_tick_in_ready_stays()` |
| Tick triggers Decision::Done at threshold | `test_tick_reaches_done()` |
| Reset returns to Ready without resetting count | `test_reset_returns_to_ready_without_resetting_count()` |
| Unhandled event in Init is dropped (no transition, count unchanged) | `test_unhandled_event_in_init_is_dropped()` |

`test_increment_count_function()` additionally covers the `increment_count` action function directly (no codegen needed).

## blox.toml

The full declarative source is `crates/bloxes/counter/blox.toml`:

```toml
[topology]
spec_imports = ["crate::DONE_AT_COUNT"]

[[topology.states]]
name = "Ready"
initial = true

[[topology.transitions]]
state = "Ready"
event = "CounterMsg::Tick(_)"
target = "stay"
actions = ["Self::count_tick"]

[[topology.transitions.guards]]
condition = "ctx.count >= DONE_AT_COUNT"
target = "done"

[[topology.transitions.guards]]
condition = "_"
target = "stay"
```

## Context Crate Dependencies

| Action function | From crate | Description |
|-------|-----------|----------------|
| `increment_count` | `blox-ctx-ticks` | Increments the `count` field |

## Related Docs

- See `spec/architecture/11-action-crate-pattern.md` for the four-layer model
- See `tokio-minimal-demo.rs` for wiring example

## Open Questions

None currently.
