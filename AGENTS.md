# Agent Orientation: Bloxide

Read this file first whenever you start a session on this repository.

## What This Project Is

Bloxide is a `no_std` hierarchical state machine (HSM) + actor messaging framework written in Rust, with first-class Embassy and Tokio runtimes. It provides a `MachineSpec` trait that domain code implements to define state topologies, event handlers, and context — all without importing any runtime. Separate runtime crates wire actors together and run them on an executor.

## Repository Layout

```
bloxide/
  spec/                        ← architecture docs, blox specs, templates (READ FIRST)
    README.md                  ← spec directory guide and SDD workflow
    architecture/              ← system design, HSM engine, messaging, wiring
    bloxes/                    ← per-blox specs (ping, pong, ...)
    templates/                 ← blox-spec.md template for new bloxes
  skills/                      ← agent skills (workflows you should follow)
    building-with-bloxide/
      SKILL.md                 ← how to build bloxes with bloxide (portable — copy to downstream projects)
      reference.md             ← deep-dive companion: macro syntax, timer/supervision patterns, worked example
    contributing-to-bloxide/
      SKILL.md                 ← how to evolve the framework: engine, runtimes, stdlib crates, macros
  crates/
    bloxide-core/              ← HSM engine, BloxRuntime trait, channel traits (no_std)
    bloxide-log/               ← feature-gated logging macros (log / defmt backends); no_std
    bloxide-macros/            ← proc macros: #[derive(BloxCtx)], transitions!, #[blox_event], etc.
    bloxide-spawn/             ← dynamic actor support: SpawnCap, PeerCtrl, HasPeers, introduce_peers (no_std)
    bloxide-timer/             ← timer library: TimerCommand, TimerId, TimerQueue, HasTimerRef, TimerService trait
    bloxide-supervisor/        ← generic reusable supervisor: SupervisorSpec, ChildGroup, ChildPolicy, GroupShutdown, LifecycleCommand
    messages/
      ping-pong-messages/      ← PingPongMsg shared by both ping and pong bloxes
      pool-messages/           ← PoolMsg, WorkerMsg, DoWork, WorkDone, etc. shared by pool and worker
      counter-messages/        ← CounterMsg shared by counter blox and minimal wiring demo
    actions/
      ping-pong-actions/       ← HasPeerRef, CountsRounds, send_initial_ping, send_pong, etc. (no concrete types)
      pool-actions/            ← WorkerSpawnFn, HasWorkers, HasWorkerFactory, HasCurrentTask, introduce_new_worker, etc.
      counter-actions/         ← CountsTicks behavior trait and increment_count action
    bloxes/
      ping/                    ← declarative Ping actor; depends on ping-pong-actions
      pong/                    ← declarative Pong actor; depends on ping-pong-actions
      worker/                  ← declarative Worker actor; depends on pool-actions (no pool-blox dependency)
      pool/                    ← declarative Pool actor; depends on pool-actions (no worker-blox dependency)
      counter/                 ← declarative Counter actor; depends on counter-actions
    impl/
      embassy-demo-impl/       ← impl crate: PingBehavior (concrete behavior for Ping)
      counter-demo-impl/       ← impl crate: CounterBehavior for tokio-minimal-demo
      tokio-pool-demo-impl/    ← impl crate: tokio worker factory for pool demo
  runtimes/
    bloxide-embassy/           ← Embassy runtime implementation
    bloxide-tokio/             ← Tokio runtime implementation; implements SpawnCap and DynamicChannelCap
  examples/
    embassy-demo.rs            ← binary: wires ping/pong actors and spawns Embassy tasks
    tokio-minimal-demo.rs      ← binary: smallest runnable layered Tokio example (counter actor)
    tokio-demo.rs              ← binary: wires ping/pong actors on Tokio
    tokio-pool-demo.rs         ← binary: wires pool/worker actors; injects spawn_worker from impl crate
  AGENTS.md                    ← this file
```

## Three Mental Models

Bloxide is easiest to understand if you keep three related mental models in your head at the same time.

| Model | Use It For | Main Pieces |
|---|---|---|
| Three-layer principle | Understanding the framework itself | Runtime, standard library crates, bloxes |
| Five-layer application structure | Organizing an application that uses Bloxide | Messages, actions, impl, blox, binary |
| Two-tier trait system | Knowing who implements which traits | Tier 1 blox-facing traits, Tier 2 runtime-facing capabilities |

### 1. Three-layer principle

This is the framework architecture described in `spec/architecture/00-layered-architecture.md`.

- Layer 1: runtime primitives and bridges
- Layer 2: standard-library crates such as `bloxide-timer`, `bloxide-supervisor`, `bloxide-spawn`
- Layer 3: blox crates that define `MachineSpec`

Use this model when you are deciding where a new capability belongs.

### 2. Five-layer application structure

This is the application-author view described in `skills/building-with-bloxide/SKILL.md` and `spec/architecture/12-action-crate-pattern.md`.

- Layer 1: messages
- Layer 2: actions
- Layer 3: impl
- Layer 4: blox
- Layer 5: binary

Use this model when you are creating or reviewing a real app that uses Bloxide.

### 3. Two-tier trait system

This is the trait boundary that keeps blox code runtime-agnostic.

- Tier 1: blox-facing traits such as `BloxRuntime`
- Tier 2: runtime-facing capabilities such as `StaticChannelCap`, `DynamicChannelCap`, `TimerService`, `SupervisedRunLoop`, `SpawnCap`

Use this model when you are wiring runtimes, reading macro output, or adding new framework capabilities.


## Where to Find Things

## Suggested Reading Order

1. **README.md** — repo map and runnable examples
2. **AGENTS.md** (this file) — mental models, key invariants, where-to-find-things table
3. **skills/building-with-bloxide/SKILL.md** — end-to-end build workflow
4. **QUICK_REFERENCE.md** — decision trees and lookup tables when you're stuck

Then dive deeper as needed:
- `spec/architecture/02-hsm-engine.md` — `MachineSpec`, dispatch, Init/start/reset
- `spec/architecture/05-handler-patterns.md` — transition patterns and `transitions!` macro
- `spec/architecture/08-supervision.md` — supervisor patterns
- `spec/architecture/11-dynamic-actors.md` — dynamic spawning and factory injection

| Question | File |
|---|---|
| What is the layered architecture and two-tier trait system? | `spec/architecture/00-layered-architecture.md` |
| How does the overall system fit together? | `spec/architecture/01-system-architecture.md` |
| How do HSMs / state machines work here? | `spec/architecture/02-hsm-engine.md` |
| How do actors send messages? | `spec/architecture/03-actor-messaging.md` |
| How are actors wired at startup? | `spec/architecture/04-static-wiring.md` |
| What are the named handler and topology patterns? | `spec/architecture/05-handler-patterns.md` |
| How do actions, logging, and the `transitions!` macro work? | `spec/architecture/06-actions.md` |
| How do typed mailboxes and priority ordering work? | `spec/architecture/07-typed-mailboxes.md` |
| How does supervision work? | `spec/architecture/08-supervision.md` |
| How is an application wired end to end? | `spec/architecture/09-application.md` |
| How do effects (timers) and capabilities work? | `spec/architecture/10-effects-and-capabilities.md` |
| **How do action crates, impl crates, and bloxes fit together?** | **`spec/architecture/12-action-crate-pattern.md`** |
| How do dynamic actors, factory injection, and peer introduction work? | `spec/architecture/11-dynamic-actors.md` |
| Spec for the Ping actor | `spec/bloxes/ping.md` |
| Spec for the Pong actor | `spec/bloxes/pong.md` |
| Spec for the Counter actor | `spec/bloxes/counter.md` |
| Spec for the Pool actor | `spec/bloxes/pool.md` |
| Spec for the Worker actor | `spec/bloxes/worker.md` |
| How does the reusable supervisor spec work? | `spec/architecture/08-supervision.md` |
| Template for a new blox | `spec/templates/blox-spec.md` |
| **How do I test a blox in isolation?** | `crates/bloxide-core/src/test_utils.rs` |
| **What is TestRuntime for?** | `crates/bloxide-core/src/test_utils.rs` |
| **How do I test timers without an executor?** | `crates/bloxide-timer/src/test_utils.rs` (`VirtualClock`) |
| **Where are the proc macro implementations?** | `crates/bloxide-macros/src/` |
| **What are the key invariants?** | This file (`AGENTS.md` → "Key Invariants") |
| **Decision trees for common tasks?** | `QUICK_REFERENCE.md` |

## Skills

Skills are reusable workflows. Read the relevant skill file before starting the corresponding task.

| Task | Skill |
|---|---|
| Building bloxes (new or modified) | `skills/building-with-bloxide/SKILL.md` |
| Evolving the framework (engine, runtimes, stdlib crates, macros) | `skills/contributing-to-bloxide/SKILL.md` |

The building guide is portable — downstream projects that depend on bloxide should copy `skills/building-with-bloxide/` into their repo and reference it from their own AGENTS.md.

## Context Definition Conventions

Context structs use naming conventions instead of explicit field annotations.
Only one annotation (`#[delegates]`) is required for behavior delegation fields.

| Field | Convention | Generates |
|-------|-----------|-----------|
| `self_id: ActorId` | Always present | `impl HasSelfId` |
| `foo_ref: ActorRef<M, R>` | Matches `HasFooRef::foo_ref()` | Auto accessor impl |
| `foo_factory: fn(...) -> ...` | Matches `HasFooFactory::foo_factory()` | Auto accessor impl |
| `behavior: B` | Must have `#[delegates(Traits)]` | Forwarding impls |

All mutable state belongs in the behavior object, not as direct context fields.

### Example

```rust
#[derive(BloxCtx)]
pub struct PingCtx<R: BloxRuntime, B: HasCurrentTimer + CountsRounds> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    
    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
```

### Field Annotation Reference

The `#[derive(BloxCtx)]` macro supports these field annotations:

- **`#[delegates(Trait1, Trait2, ...)]`** — Required for behavior fields. Generates forwarding impls
  to the inner type. Traits must be marked with `#[delegatable]` in their definition crate.

**Deprecated (auto-detected by convention):**
- ~~`#[self_id]`~~ — Auto-detected from `self_id: ActorId` field
- ~~`#[provides(Trait)]`~~ — Auto-detected from `_ref` field naming convention
- ~~`#[ctor]`~~ — Auto-detected for non-`_ref` fields (factories, etc.)


1. **`bloxide-core` is `no_std`** — zero OS, Tokio, or Embassy imports. `futures-core` is the only always-on runtime library dep; optional instrumentation deps (such as feature-gated `tracing`) must remain `no_std` compatible. Proc-macro crates (e.g., `bloxide-macros`) are exempt — they compile for the host and have no `no_std` impact.
2. **Blox crates are runtime-agnostic** — generic over `R: BloxRuntime`. Never import `bloxide-embassy` or any executor from a blox crate.
3. **No runtime types in messages** — domain message enums contain plain data only; no `ActorRef`, no raw senders/receivers.
4. **Shared messages in dedicated crates** — message types used by two or more blox crates live in a `*-messages` crate to avoid circular dependencies.
5. **Only leaf states as transition targets** — the engine `debug_assert`s this; violating it in release is UB.
6. **`on_entry` / `on_exit` are infallible** — they are `fn(&mut Ctx)` with no `Result`. Fallible work belongs in a `TransitionRule`'s `actions` function or is deferred to the target state's `on_entry`.
7. **Actions before guards** — event handlers use `TransitionRule { matches, actions, guard }`. All side effects go in `actions: fn(&mut Ctx, &Event)`. Guards are pure: `guard: fn(&Ctx, &ActionResults, &Event) -> Guard`. The borrow checker enforces this — `guard` receives `&Ctx` and `&ActionResults`, not `&mut Ctx`.
8. **Bubbling is implicit** — states with no matching rule automatically bubble to the parent. Empty `transitions: &[]` means all events bubble. Never add a catch-all rule that manually returns a parent; bubbling happens automatically when no rule matches.
9. **Blox crates never import impl crates** — concrete types are only referenced by the binary. Blox crates depend on actions crates (traits + generic functions) only.
10. **Action crates are portable interface layers** — action crates (`*-actions`) define traits and trait-bounded generic functions. They may contain portable generic action logic and `bloxide-log` macros, but no runtime-specific imports, file I/O, or Embassy/Tokio code.
11. **Use named struct variants in message enums** — `PingPongMsg::Ping(Ping { round })` not `PingPongMsg::Ping(u32)`. Named fields are accessible by name across module boundaries without positional fragility.
12. **Lifecycle commands flow through dispatch() at VirtualRoot level** — actors handle them as domain events via `root_transitions()`. The VirtualRoot intercepts `LifecycleCommand` variants (Start, Reset, Stop, Ping) before they reach user-declared states. `Start` exits Init and enters `initial_state()`; `Reset` goes to user-defined `initial_state()` (actor immediately operational); `Stop` goes to Init (suspended, can be restarted with `Start`); `Kill` immediately aborts the task (permanently dead). Actors never implement `is_start()` or call `machine.start()`/`machine.reset()` explicitly — lifecycle is driven entirely by dispatch. `root_transitions()` returns `&[]` for supervised actors (lifecycle handled by VirtualRoot, not user code).
13. **`is_error` takes precedence over `is_terminal`** — if a state returns `true` for both `is_error()` and `is_terminal()`, the runtime reports only `ChildLifecycleEvent::Failed` (not `Done`). Use `is_error` for fault states that should trigger supervisor intervention, and `is_terminal` for normal completion.
14. **Logging via `bloxide-log` macros** — use `blox_log_info!`, `blox_log_debug!`, etc. from `bloxide-log`. Logging is a compile-time feature flag (`log` or `defmt`), not a runtime trait. Never add a `Logs` trait or logger generic parameter to blox contexts.

15. **Dynamic actor spawning via factory injection** — Blox crates never declare `R: SpawnCap`. Dynamic spawning uses factory injection via `#[ctor]` fields in blox context structs. The binary (or impl crate) provides the concrete factory closure at construction time. This keeps blox crates portable across all runtimes, including Embassy which lacks `SpawnCap`.
16. **KillCap is a runtime capability, not a message** — `supervisor.kill(child_id)` immediately aborts the child's task without any callbacks firing. No `on_exit` handlers run; the task is dropped in-place. KillCap is for (1) unresponsive actors that cannot process Stop, or (2) cleanup of stopped actors whose resources should be freed immediately. Kill works for both static and dynamic actors; killed actors are permanently dead and cannot be restarted — normal lifecycle uses Reset/Stop through dispatch(). KillCap lives in `bloxide-core` as a trait; runtimes implement it (e.g., `TokioKillCap` wraps `JoinHandle::abort`). Supervisors hold a `KillCap` reference; actors never see it.

## Development Workflow

1. **Spec first** — Write/update `spec/bloxes/<name>.md` with state diagram, events, transitions
2. **Tests next** — Write `TestRuntime`-based tests per acceptance criteria
3. **Then code** — Implement `MachineSpec` to pass tests
4. **Review** — Verify impl matches spec; update tests if gaps found
5. **Keep in sync** — Update spec diagrams if implementation reveals spec errors

See `skills/building-with-bloxide/SKILL.md` for the full step-by-step workflow.

## Clarification: Factory Injection and Supervision

If you're confused about `#[ctor]` fields or why supervised actors return `&[]` for `root_transitions()`, read `spec/architecture/13-factory-injection-and-supervision.md`. It contains:

- **Layer-by-layer walkthrough** of factory injection for dynamic spawning
- **How `#[ctor]` works** and what code it generates
- **Why lifecycle events bypass the actor's handler table** (two-stream architecture)
- **Decision trees** for choosing field annotations
