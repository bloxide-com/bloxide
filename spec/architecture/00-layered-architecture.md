# Layered Architecture

This document defines the foundational architecture of Bloxide: the three-layer principle, the two-tier trait system, and the decision rule for classifying new capabilities.

## Three-Layer Principle

```
Layer 3: Bloxes
  HSM specs using accessor traits + action functions.
  Generic over R: BloxRuntime. Never import runtime code.

Layer 2: Standard Library (patterns)
  Message types, accessor traits, action functions, shared data structures,
  and runtime-facing service traits.
  Only depend on BloxRuntime. Crates: bloxide-timer, bloxide-supervisor.

Layer 1: Runtime (primitives + bridges)
  Primitives: channels (BloxRuntime), native timers, spawning, I/O.
  Bridges: service trait impls connecting Layer 1 primitives to Layer 2 contracts.
  Crates: bloxide-embassy (and future runtimes).
```

## Two-Tier Trait System

Traits serve two audiences. Blox crates only see Tier 1.

### Tier 1 — Blox-facing

- `BloxRuntime` (in `bloxide-core`) — the sole trait bloxes are generic over. Defines `Sender`, `Receiver`, `Stream`, `to_stream`, `send_via`, `try_send_via`.

### Tier 2 — Wiring/runtime-facing

These traits formalize the contract that runtime crates must fulfill. They enable trait-qualified dispatch in macros, and give compile-time errors if a runtime forgets to implement a required service. They are NEVER used as bounds on blox crates.

- `StaticChannelCap: BloxRuntime` (in `bloxide-core`) — compile-time capacity channel creation. Used by `channels!` macro.
- `DynamicChannelCap: BloxRuntime` (in `bloxide-core`) — runtime-configurable channel creation. Used by `TestRuntime`.
- `TimerService: BloxRuntime` (in `bloxide-timer`) — timer service run loop. Each runtime bridges `TimerQueue` to its native timer.
- `SupervisedRunLoop: BloxRuntime` (in `bloxide-supervisor`) — supervised actor run loop. Each runtime merges lifecycle commands with domain mailboxes.

### Standard Library Crate Pattern

Each standard library crate defines both sides:

- **Blox-facing**: messages, accessor traits, action functions, shared data structures
- **Runtime-facing**: a service trait that runtimes implement

Example with `bloxide-timer`:
- Blox-facing: `TimerCommand`, `TimerId`, `HasTimerRef<R>`, `set_timer()`, `cancel_timer()`, `TimerQueue`
- Runtime-facing: `TimerService` trait

## Decision Rule

When adding something new, ask: **does it require async waiting on something other than messages?**

- **No** (synchronous hardware, pure computation) → context field, handlers use directly
- **Messages only** (domain actors) → standard run loop (`run_root` / `run_supervised_actor`)
- **Messages + external source** (timers, UART, network) → standard library crate defining messages + actions + data structures + service trait; runtime crate implements the trait bridging its native primitives

## Dependency Graph

```
bloxide-core (BloxRuntime, StaticChannelCap, DynamicChannelCap, HSM engine)
  └── bloxide-macros (proc macros; host-only, no_std safe)

bloxide-log (feature-gated logging: log / defmt / no-op)
  No dependency on bloxide-core — standalone crate consumed directly by blox crates.

bloxide-timer (depends on bloxide-core)
  Blox-facing: TimerCommand, TimerQueue, HasTimerRef, set_timer, cancel_timer
  Runtime-facing: trait TimerService

bloxide-supervisor (depends on bloxide-core)
  Blox-facing: LifecycleCommand, ChildLifecycleEvent, ChildGroup, HasChildren, actions
  Runtime-facing: trait SupervisedRunLoop

bloxide-embassy (runtime crate; depends on bloxide-core, bloxide-timer, bloxide-supervisor)
  impl BloxRuntime + StaticChannelCap + TimerService + SupervisedRunLoop
  macros: channels!, next_actor_id!, actor_task!, actor_task_supervised!, root_task!,
          timer_task!, spawn_child!, spawn_timer!
```
