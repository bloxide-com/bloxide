# Layered Architecture

> **When would I use this?** Use this document when deciding where a new
> capability belongs in the framework, or when you need the canonical reference
> for the two-tier trait system (Tier 1 vs Tier 2 traits).

This document defines the foundational architecture of Bloxide: the three-layer principle, the two-tier trait system, and the decision rule for classifying new capabilities.

## Three-Layer Principle

```
Layer 3: Bloxes
  HSM specs using accessor traits + action functions.
  Generic over R: BloxRuntime. Never import runtime code.

Layer 2: Standard Library (patterns)
  Message types, accessor traits, action functions, shared data structures,
  and runtime-facing service traits.
  Only depend on BloxRuntime. Crates: bloxide-timer, bloxide-child-management,
  bloxide-supervisor, bloxide-spawn, bloxide-peers, bloxide-messaging.

Layer 1: Runtime (primitives + bridges)
  Primitives: channels (BloxRuntime), native timers, spawning, I/O.
  Bridges: service trait impls connecting Layer 1 primitives to Layer 2 contracts.
  Crates: bloxide-embassy, bloxide-tokio.
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
- `SpawnCap: DynamicChannelCap` (in `bloxide-spawn`) — dynamic actor spawning. Extends `DynamicChannelCap` for runtimes that can spawn futures at runtime (Tokio, TestRuntime).
- `KillCapability: BloxRuntime` (in `bloxide-core`) — runtime capability for immediately aborting actor tasks. Used by supervisors for policy-driven cleanup of dynamic actors.

### Standard Library Crate Pattern

Each standard library crate defines both sides:

- **Blox-facing**: messages, accessor traits, action functions, shared data structures
- **Runtime-facing**: a service trait that runtimes implement

Example with `bloxide-timer`:
- Blox-facing: `TimerCommand`, `TimerId`, `HasTimerRef<R>`, `set_timer()`, `cancel_timer()`, `TimerQueue`
- Runtime-facing: `TimerService` trait

## Decision Rule

When adding something new, ask: **does it require async waiting on something other than messages?**

- **No** (synchronous hardware, pure computation) → context field, handlers use directly.  
  Example: store a GPIO handle or checksum calculator in `Ctx` and call it from actions.
- **Messages only** (domain actors) → standard run loop (`run_root` / `run_supervised_actor`).  
  Example: ping/pong request-response flow with no timers or external service loop.
- **Messages + external source** (timers, UART, network) → standard library crate defining messages + actions + data structures + service trait; runtime crate implements the trait bridging its native primitives.  
  Example: `bloxide-timer` (`TimerCommand`, `set_timer`, `TimerQueue`, `TimerService`).

## Dependency Graph

```
bloxide-core (BloxRuntime, StaticChannelCap, DynamicChannelCap, HSM engine)
  [re-exports from] bloxide-macros (proc macros; host-only, no_std safe)

Note: bloxide-macros depends only on syn, quote, proc-macro2 (not bloxide-core).
bloxide-core re-exports derive macros for blox crates.

bloxide-log (feature-gated logging: log / defmt / no-op)
  No dependency on bloxide-core — standalone crate consumed directly by blox crates.

bloxide-timer (depends on bloxide-core)
  Blox-facing: TimerCommand, TimerQueue, HasTimerRef, set_timer, cancel_timer
  Runtime-facing: trait TimerService

bloxide-supervisor (depends on bloxide-core, bloxide-child-management, bloxide-spawn)
  Blox-facing: SupervisorSpec, SupervisorControl, RegisterChild, action functions
  Runtime-facing: trait SupervisedRunLoop

bloxide-child-management (depends on bloxide-core)
  ChildGroup, ChildEntry, ChildPhase, HasChildGroup, RestartStrategy

bloxide-spawn (depends on bloxide-core)
  SpawnCap, SpawnFn, SpawnOutput, ChildRegistrar, spawn_child helper

bloxide-peers (depends on bloxide-core)
  Peer introduction: PeerCtrl, AddPeer, RemovePeer, HasPeers, introduce_peers

bloxide-messaging (depends on bloxide-core)
  Accessor traits: HasSelfRef<R,M>, HasPeerRef<R,M> for peer/self messaging

bloxide-embassy (runtime crate; depends on bloxide-core, bloxide-timer, bloxide-supervisor, bloxide-child-management)
  impl BloxRuntime + StaticChannelCap + TimerService
  macros: channels!, next_actor_id!, actor_task!, actor_task_supervised!, root_task!,
          timer_task!, spawn_child!, spawn_timer!
  Note: StaticChannelCap only (no DynamicChannelCap, no SpawnCap).

bloxide-tokio (runtime crate; depends on bloxide-core, bloxide-timer, bloxide-supervisor, bloxide-child-management, bloxide-spawn)
  impl BloxRuntime + DynamicChannelCap + TimerService + SpawnCap + KillCapability
  macros: channels!, next_actor_id!, actor_task!, actor_task_supervised!, spawn_timer!, spawn_child_dynamic!
```

## Tier 2 Implementation Map

This table shows which runtime implements each Tier 2 capability.

|| Capability | Tier 2 Trait | bloxide-embassy | bloxide-tokio | TestRuntime | Notes |
||------------|--------------|-----------------|---------------|-------------|-------|
|| Static channel creation | `StaticChannelCap` | ✅ | ❌ | ❌ | Compile-time capacity via `channels!` (Embassy only) |
|| Dynamic channel creation | `DynamicChannelCap` | ❌ | ✅ | ✅ | Runtime-configurable capacity; Tokio uses `__dyn_channels_proc_macro` |
|| Timer service | `TimerService` | ✅ | ✅ | ❌ | Bridges native timer to `TimerQueue` |
|| Spawn capability | `SpawnCap` | ❌ | ✅ | ✅ | Dynamic actor spawning |
|| Kill capability | `KillCapability` | ❌ | ✅ | ❌ | Immediately aborts actor tasks for dynamic actor cleanup |

### Feature Flags

| Runtime | Feature | Enables |
|---------|---------|---------|
| bloxide-embassy | (default) | `StaticChannelCap`, `TimerService` |
| bloxide-tokio | (default) | `TimerService` |
| bloxide-tokio | `dynamic` | `DynamicChannelCap`, `SpawnCap` (via `bloxide-spawn`) |

### TestRuntime (in bloxide-core)

TestRuntime implements `DynamicChannelCap` and `SpawnCap` for test ergonomics. `DynamicChannelCap` is in `bloxide-core`; `SpawnCap` is in `bloxide-spawn` (the `SpawnCap` impl for `TestRuntime` is gated behind the `std` feature). This keeps capabilities in their own crates while allowing tests to exercise dynamic spawning without a real executor.

### Tier 2 Trait Naming Convention

| Suffix | When to Use | Examples |
|--------|-------------|----------|
| `*Service` | Async bridge traits that run a background task | `TimerService` |
| `*RunLoop` | Traits that merge multiple streams into an actor loop | `SupervisedRunLoop` |
| `*Cap` (Capability) | Traits that provide runtime capabilities for injection | `SpawnCap`, `StaticChannelCap`, `DynamicChannelCap` |

**Why different suffixes?**
- `*Service` traits are async services (like timer management)
- `KillCapability` traits provide immediate task termination for dynamic actor cleanup
- `*Cap` traits are capabilities that runtimes implement for injection (spawning, channels)
