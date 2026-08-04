# Layered Architecture

> **When would I use this?** Use this document when deciding where a new
> capability belongs in the framework, or when you need the canonical reference
> for the two-tier trait system (Tier 1 vs Tier 2 traits).

This document defines the foundational architecture of Bloxide: the three-layer principle, the two-tier trait system, and the decision rule for classifying new capabilities.

> **NO BACKWARDS COMPATIBILITY.** This project does not preserve backwards
> compatibility — not for APIs, CLIs, config formats, terminology, or
> architecture layers. When something changes, update every call site, every
> doc, and every fixture in the same change, and delete the old form
> completely. Never add legacy aliases, deprecation periods, compatibility
> shims, feature bridges, or "kept for backwards compatibility" notes.

## Suggested Reading Path

The docs in this directory are numbered 00–20 but are not meant to be read strictly in order. For a first read, follow this path:

1. [02 — HSM Engine](02-hsm-engine.md) (engine + lifecycle)
2. [03 — Actor Messaging](03-actor-messaging.md)
3. [05 — Handler Patterns](05-handler-patterns.md)
4. [08 — Supervision](08-supervision.md)
5. [13 — Factory Injection](13-factory-injection-and-supervision.md)
6. [15 — Composable Context Crates](15-composable-context-crates.md)
7. [18 — Spawn Architecture](18-spawn-architecture.md)

The remaining docs (04, 06, 07, 09, 10, 11, 12, 16, 17, 19, 20) are reference material — read them as needed when working on a related area.

## Three-Layer Principle

```
Layer 3: Bloxes
  HSM specs using action functions.
  Generic over R: BloxRuntime. Never import runtime code.

Layer 2: Standard Library (patterns)
  Message types, action functions, shared data structures,
  and runtime-facing service traits.
  Only depend on BloxRuntime. Crates: bloxide-timer, bloxide-child-management,
  bloxide-supervisor, bloxide-spawn, bloxide-peers, blox-ctx-ping-pong.

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

- **Blox-facing**: messages, action functions, shared data structures
- **Runtime-facing**: a service trait that runtimes implement

Example with `bloxide-timer`:
- Blox-facing: `TimerCommand`, `TimerId`, `set_timer()`, `cancel_timer()`, `TimerQueue`
- Runtime-facing: `TimerService` trait

## Decision Rule

When adding something new, ask: **does it require async waiting on something other than messages?**

- **No** (synchronous hardware, pure computation) → context field, handlers use directly.  
  Example: store a GPIO handle or checksum calculator in `Ctx` and call it from actions.
- **Messages only** (domain actors) → standard run loop (`run` with `RunConfig::root` / `run` with `RunConfig::supervised`).  
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
  No dependency on bloxide-core — consumed by bloxide-core itself, the runtime
  crates, and context crates. Blox crates never depend on it (see AGENTS.md
  invariant #15).

bloxide-timer (depends on bloxide-core)
  Blox-facing: TimerCommand, TimerQueue, set_timer, cancel_timer, cancel_timer_by_id
  Runtime-facing: trait TimerService

bloxide-supervisor (depends on bloxide-core, bloxide-child-management)
  The supervisor blox — reference consumer of the child-management platform
  feature (spec 20). Owns only: blox.toml + generated spec
  (SupervisorSpec, SupervisorCtx, SupervisorEvent) + concrete_spec.rs (test
  fixture) + tests. ChildCtrl/RegisterChild/RegisterDynamicChild and the
  supervision action functions live in bloxide-child-management.
  Runtime-facing: none — the supervised run loop is the unified `run()` function
  with `RunConfig` in bloxide-core, re-exported by each runtime

bloxide-child-management (depends on bloxide-core)
  ChildGroup, ChildEntry, ChildPhase, ChildGroupBuilder, ChildPolicy,
  GroupShutdown, ChildCtrl/RegisterChild/RegisterDynamicChild control plane,
  supervision action functions

bloxide-spawn (depends on bloxide-core)
  SpawnCap, SpawnFn, SpawnOutput, ChildCtrlRegistrar, spawn_child helper,
  Kill type (KillCapability impl for SpawnCap runtimes)

bloxide-peers (depends on bloxide-core)
  Peer introduction: PeerCtrl, AddPeer, RemovePeer, introduce_peers,
  broadcast_to_peers (generic, M: Clone), apply_peer_control

blox-ctx-ping-pong (depends on bloxide-core, bloxide-timer, ping-pong-messages)
  Action functions for peer/self messaging (send_ping, send_pong,
  send_initial_ping, schedule_resume)

bloxide-embassy (runtime crate; depends on bloxide-core, bloxide-timer, bloxide-child-management)
  impl BloxRuntime + StaticChannelCap + TimerService
  macros: channels!, next_actor_id!, actor_task!, actor_task_supervised!, root_task!,
          timer_task!, spawn_child!, spawn_timer!
  Note: StaticChannelCap only (no DynamicChannelCap, no SpawnCap). Re-exports
  the shared ChildGroupBuilder from bloxide-child-management (crate root and
  prelude), reaching static channels via the GroupChannelCap impl in
  mailbox.rs, so generated wiring is identical across runtimes.

bloxide-tokio (runtime crate; depends on bloxide-core, bloxide-timer, bloxide-child-management, bloxide-spawn)
  impl BloxRuntime + DynamicChannelCap + TimerService + SpawnCap + KillCapability
  macros: channels!, next_actor_id!, actor_task!, actor_task_supervised!, spawn_timer!, spawn_child!
```

## Tier 2 Implementation Map

This table shows which runtime implements each Tier 2 capability.

|| Capability | Tier 2 Trait | bloxide-embassy | bloxide-tokio | TestRuntime | Notes |
||------------|--------------|-----------------|---------------|-------------|-------|
|| Static channel creation | `StaticChannelCap` | ✅ | ❌ | ❌ | Compile-time capacity via `channels!` (Embassy only) |
|| Dynamic channel creation | `DynamicChannelCap` | ❌ | ✅ | ✅ | Runtime-configurable capacity; Tokio uses `__dyn_channels_proc_macro` |
|| Timer service | `TimerService` | ✅ | ✅ | ❌ | Bridges native timer to `TimerQueue`; tests use `VirtualClock` instead |
|| Spawn capability | `SpawnCap` | ❌ | ✅ | ✅ | Dynamic actor spawning |
|| Kill capability | `KillCapability` | ❌ (`NoKill`) | ✅ (`Kill`) | ✅ (`Kill`, recorded) | Immediately aborts actor tasks for dynamic actor cleanup; TestRuntime's `kill` records the handle in a thread-local log (no real tasks) — tests assert via `drain_killed()` / `kill_count()` |

### Feature Flags

| Crate | Feature | Enables |
|-------|---------|---------|
| bloxide-embassy | `std` | std-target testing (default is `no_std`) |
| bloxide-tokio | _(none)_ | All capabilities are unconditional: `DynamicChannelCap`, `TimerService`, `SpawnCap`, `KillCapability` (`Kill`) |
| pool-blox, tokio-pool-demo-impl | `dynamic` | Dynamic-spawn wiring (spawn factory injection, dynamic actor registration in system.toml) |

### TestRuntime (in runtimes/bloxide-test-runtime)

TestRuntime implements `DynamicChannelCap` (from `bloxide-core`) and `SpawnCap`
(from `bloxide-spawn`) for test ergonomics. This keeps capabilities in their
own crates while allowing tests to exercise dynamic spawning without a real
executor. It is intentionally not a full-fidelity runtime: channel capacity
**is** enforced for `try_send` (the backpressure path action functions use),
but `send_via` is unbounded and never fails; `SpawnCap` handles are `usize`
spawn ids (`KillHandle = usize`) and `kill` records the id in a thread-local
log — tests assert via `drain_killed()` / `kill_count()` — since TestRuntime
runs no real tasks. Receivers model all-streams-close semantics (issue #134):
dropping the last sender wakes the receiver, which drains queued envelopes
and then returns `Poll::Ready(None)`. Tests validate HSM logic, not
runtime-integration behavior.

### Tier 2 Trait Naming Convention

| Suffix | When to Use | Examples |
|--------|-------------|----------|
| `*Service` | Async bridge traits that run a background task | `TimerService` |
| `*Cap` (Capability) | Traits that provide runtime capabilities for injection | `SpawnCap`, `StaticChannelCap`, `DynamicChannelCap`, `GroupChannelCap` |

**Why different suffixes?**
- `*Service` traits are async services (like timer management)
- `*Cap` traits are capabilities that runtimes implement for injection (spawning, channels)

Note: the actor run loop is no longer a trait — it is the unified `run()`
function in `bloxide-core`, configured by `RunConfig` (root / supervised /
supervised_with_abort / unsupervised / bare) and re-exported by each runtime.
`RunConfig` carries `exit_on_stop` / `exit_on_fail` flags: supervised actors
set both to `false` (the task stays alive on `Stopped` and `Failed`; the
supervisor's `ChildPolicy` decides what happens next), while
root/unsupervised/bare actors set both to `true`.

## System Overview

Bloxide targets embedded systems (Embassy) and server environments (Tokio)
while remaining runtime-agnostic and fully testable without an executor
(`runtimes/bloxide-test-runtime`).

**Key rule**: domain crates (messages, context, bloxes) depend on
`bloxide-core` and standard-library crates only — never on a runtime crate.
Runtime internals never appear in blox code.

### Separation of Concerns

| Layer | What it contains | What it must NOT contain |
|-------|-----------------|--------------------------|
| Messages crates | Plain data enums/structs | Runtime types, `ActorRef` |
| Context crates (`blox-ctx-*`, `blox-ctx-ping-pong`) | Free action functions taking concrete params | Runtime imports, file I/O |
| Blox crates | `blox.toml` + generated `MachineSpec` impl, `Ctx`, `Event` enum | Runtime imports, executor types, Rust logic |
| `bloxide-core` | `MachineSpec`, `StateMachine`, `ActorRef`, `BloxRuntime`, `StaticChannelCap`, `DynamicChannelCap`, `GroupChannelCap`, `Mailboxes`, `run`/`RunConfig` | Tokio, Embassy, OS imports |
| `bloxide-timer` | `TimerCommand`, `TimerId`, `TimerQueue`, `set_timer`, `cancel_timer`, `TimerService` trait | Runtime imports, executor types |
| `bloxide-supervisor` | `SupervisorSpec`, `SupervisorCtx` (blox.toml + generated + `concrete_spec.rs` test fixture + tests only) | Runtime imports, executor types |
| `bloxide-child-management` | `ChildGroup`, `ChildEntry`, `ChildPhase`, `ChildGroupBuilder`, `ChildPolicy`, `GroupShutdown`, `ChildCtrl`/`RegisterChild`/`RegisterDynamicChild`, action functions | Runtime imports, executor types |
| `bloxide-spawn` | `SpawnCap`, `SpawnFn`, `SpawnOutput`, `ChildCtrlRegistrar`, `spawn_child` helper | Runtime imports, executor types |
| `bloxide-peers` | `PeerCtrl`, `AddPeer`, `RemovePeer`, `introduce_peers`, `broadcast_to_peers` | Runtime imports, executor types |
| Runtime crates | `BloxRuntime` + channel-capability + `TimerService` impls, actor task macros | Domain logic |
| Application/Wiring | `system.toml` + generated `main.rs`: channel creation, `ActorRef` injection, task spawning | Business logic |

### Multi-Mailbox Model

Each actor has **one typed mailbox per message type** it can receive. The actor's
`Event` enum wraps all receivable types. The `Mailboxes` trait selects across them
in priority order. See [07-typed-mailboxes.md](07-typed-mailboxes.md).

### Supervision

A supervisor is a reusable `MachineSpec` provided by `bloxide-supervisor`. It
receives `ChildLifecycleEvent` from child run loops (generated by observing
`DispatchOutcome` in `run()`) and applies its configured `ChildPolicy` with
`GroupShutdown` via `ChildGroup<R>`. See [08-supervision.md](08-supervision.md).

### Runtime Selection

| Runtime | Best for | Features |
|---------|----------|----------|
| `bloxide-embassy` | Embedded systems, `no_std` targets | `StaticChannelCap`, `TimerService` |
| `bloxide-tokio` | Server applications, native targets | `DynamicChannelCap`, `TimerService`, `SpawnCap`, `Kill` |
| `TestRuntime` | Unit tests, no executor needed | `DynamicChannelCap`, `SpawnCap` (fidelity limits noted above) |

### Feature Flags (`bloxide-core`)

| Flag | Enables | Default |
|------|---------|---------|
| _(none)_ | `no_std` core | ✓ |
| `alloc` | `extern crate alloc` — heap-backed collections where needed | — |
| `std` | `extern crate std`; implies `alloc` (used by std-target runtimes and host tests) | — |
| `tracing` | `tracing::trace!` hooks in the engine (`trace_on_entry!` etc.) | — |

### Application Layout

Applications follow the four-layer structure — messages crates, context
crates (action functions), blox crates (declarative), and the binary
(`system.toml` + generated `main.rs` + optional impl crates). See
[12-action-crate-pattern.md](12-action-crate-pattern.md) and
[16-declarative-wiring.md](16-declarative-wiring.md).
