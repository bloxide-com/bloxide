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
    bloxide-core/              ← HSM engine, BloxRuntime trait, channel traits, messaging data types (ActorRef, Envelope, ActorId) (no_std)
    bloxide-log/               ← feature-gated logging macros (log / defmt backends); no_std
    bloxide-macros/            ← proc macros: #[blox_event], etc.
    bloxide-messaging/         ← service crate: action functions (send_ping, send_pong, etc.) for peer/self messaging
    bloxide-timer/             ← timer library: TimerCommand, TimerId, TimerQueue, TimerService trait
    bloxide-child-management/  ← reusable child tracking: ChildGroup, ChildEntry, ChildPhase (platform primitive)
    bloxide-supervisor/        ← supervisor blox: SupervisorSpec, SupervisorControl, RegisterChild, action functions; re-exports ChildGroup/ChildPolicy/etc. from bloxide-child-management
    bloxide-peers/             ← peer introduction: PeerCtrl<M,R>, AddPeer, RemovePeer, introduce_peers, broadcast_to_peers
    bloxide-spawn/             ← spawn capability: SpawnFn, SpawnOutput, SpawnCap (platform primitive)
    messages/
      ping-pong-messages/      ← PingPongMsg shared by both ping and pong bloxes
      pool-messages/           ← PoolMsg, WorkerMsg, DoWork, WorkDone, etc. shared by pool and worker
      counter-messages/        ← CounterMsg shared by counter blox and minimal wiring demo
      bhsm-tst-messages/       ← BhsmTstMsg shared by the bhsm-tst HSM topology demo
    context/
      blox-ctx-workers/        ← domain context crate + action functions
      blox-ctx-pool-ref/       ← domain context crate + notify_pool_done action function
      blox-ctx-rounds/         ← increment_round action function
      blox-ctx-current-timer/  ← schedule_resume/cancel_timer_by_id action functions
      blox-ctx-current-task/   ← domain context crate
      blox-ctx-ticks/          ← increment_count action function
    bloxes/
      ping/                    ← declarative Ping actor; depends on context crates (blox-ctx-rounds, blox-ctx-current-timer, bloxide-messaging)
      pong/                    ← declarative Pong actor; depends on context crates (bloxide-messaging)
      worker/                  ← declarative Worker actor; depends on context crates (blox-ctx-pool-ref, bloxide-peers)
      pool/                    ← declarative Pool actor; depends on context crates (blox-ctx-pool-ref, bloxide-peers)
      counter/                 ← declarative Counter actor; depends on context crates (blox-ctx-ticks)
      bhsm-tst/                ← declarative Miro Samek HSM test blox (states S/S1/S11/S2/S21/S211)
    impl/
      tokio-pool-demo-impl/    ← impl crate: free functions (process_work, spawn_worker, pool action handlers) for pool demo
    tools/
      bloxide-codegen/         ← TOML-driven code generator library
      cargo-blox/              ← CLI: cargo blox generate / new / build / check / test / run
  tools/
    bloxide-viz-export/        ← source-to-JSON exporter for visualizer
    bloxide-visualizer/        ← browser-based state-machine visualizer
  runtimes/
    bloxide-embassy/           ← Embassy runtime implementation
    bloxide-tokio/             ← Tokio runtime implementation; implements SpawnCap and DynamicChannelCap
    bloxide-test-runtime/      ← test runtime for executor-free unit testing (no async, no spawning)
  apps/
    embassy-demo/             ← system.toml + generated main.rs: ping/pong on Embassy
    tokio-demo/               ← system.toml + generated main.rs: ping/pong on Tokio
    tokio-minimal-demo/       ← system.toml + generated main.rs: counter actor on Tokio
    tokio-pool-demo/          ← system.toml + generated main.rs: pool/worker on Tokio
  AGENTS.md                    ← this file
```

## Three Mental Models

Bloxide is easiest to understand if you keep three related mental models in your head at the same time.

| Model | Use It For | Main Pieces |
|---|---|---|
| Three-layer principle | Understanding the framework itself | Runtime, standard library crates, bloxes |
| Four-layer application structure | Organizing an application that uses Bloxide | Messages, context, blox, binary |
| Two-tier trait system | Knowing who implements which traits | Tier 1 blox-facing traits, Tier 2 runtime-facing capabilities |

### 1. Three-layer principle

This is the framework architecture described in `spec/architecture/00-layered-architecture.md`.

- Layer 1: runtime primitives and bridges
- Layer 2: standard-library crates such as `bloxide-timer`, `bloxide-supervisor`
- Layer 3: blox crates that define `MachineSpec`

Use this model when you are deciding where a new capability belongs.

### 2. Four-layer application structure

This is the application-author view described in `skills/building-with-bloxide/SKILL.md` and `spec/architecture/12-action-crate-pattern.md`.

```
Before:  messages → actions → context → impl → blox → (codegen) → binary
After:   messages → context (includes actions) → blox → binary
         impl = optional reusable behavior libraries
```

- Layer 1: messages — shared plain-data enums/structs
- Layer 2: context — free action functions taking concrete params (was: actions + context + impl)
- Layer 3: blox — purely declarative topology, no logic
- Layer 4: binary — system.toml + two-stage codegen + optional impl crates

Use this model when you are creating or reviewing a real app that uses Bloxide.

### 3. Two-tier trait system

This is the trait boundary that keeps blox code runtime-agnostic.

- Tier 1: blox-facing traits such as `BloxRuntime`
- Tier 2: runtime-facing capabilities such as `StaticChannelCap`, `DynamicChannelCap`, `TimerService`, `SupervisedRunLoop`, `SpawnCap`

With the elimination of accessor traits and the `B` generic,
context structs are plain structs with plain fields. Action functions
take concrete params extracted from the context fields. The codegen
emits wrapper closures that extract fields and call the action functions.

Use this model when you are wiring runtimes, reading macro output, or adding new framework capabilities.


## Where to Find Things

## Suggested Reading Order

1. **README.md** — repo map and runnable examples
2. **AGENTS.md** (this file) — mental models, key invariants, where-to-find-things table
3. **skills/building-with-bloxide/SKILL.md** — end-to-end build workflow
4. **QUICK_REFERENCE.md** — decision trees and lookup tables when you're stuck

Then dive deeper as needed:
- `spec/architecture/02-hsm-engine.md` — `MachineSpec`, dispatch, Init/start/reset
- `spec/architecture/05-handler-patterns.md` — transition patterns and declarative TOML transitions
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
| How do actions, logging, and declarative TOML transitions work? | `spec/architecture/06-actions.md` |
| How do typed mailboxes and priority ordering work? | `spec/architecture/07-typed-mailboxes.md` |
| How does supervision work? | `spec/architecture/08-supervision.md` |
| How is an application wired end to end? | `spec/architecture/09-application.md` |
| How do effects (timers) and capabilities work? | `spec/architecture/10-effects-and-capabilities.md` |
| **How do context crates, impl crates, and bloxes fit together?** | **`spec/architecture/12-action-crate-pattern.md`** |
| How do dynamic actors, factory injection, and peer introduction work? | `spec/architecture/11-dynamic-actors.md` |
| How does factory injection interact with supervision? | `spec/architecture/13-factory-injection-and-supervision.md` |
| How does the unified lifecycle model work? | `spec/architecture/14-unified-lifecycle.md` |
| How do composable context crates work? | `spec/architecture/15-composable-context-crates.md` |
| How does declarative wiring and handle injection work? | `spec/architecture/16-declarative-wiring.md` |
| How does blox.toml serve as the source of truth? | `spec/architecture/17-blox-toml-source-of-truth.md` |
| How does spawning work? | `spec/architecture/18-spawn-architecture.md` |
| Spec for the Ping actor | `spec/bloxes/ping.md` |
| Spec for the Pong actor | `spec/bloxes/pong.md` |
| Spec for the Counter actor | `spec/bloxes/counter.md` |
| Spec for the Pool actor | `spec/bloxes/pool.md` |
| Spec for the Worker actor | `spec/bloxes/worker.md` |
| Spec for the BHSM test actor | `spec/bloxes/bhsm.md` |
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

Context fields are defined via `[[context.fields]]` and `[[context.uses]]` entries in `blox.toml`.
The codegen auto-emits `self_id: ActorId` (always, first field).
State fields are declared directly in the context struct. No annotations — plain struct fields.

| Field | Source | Generates |
|-------|--------|-----------|
| `self_id: ActorId` | Auto-emitted by codegen (always) | Plain field |
| `foo_ref: ActorRef<M, R>` | `[[context.uses]]` with `field = "foo_ref"` | Plain field |
| `foo_factory: fn(...) -> ...` | `[[context.uses]]` with `role = "ctor"` | Constructor parameter |
| `round: u32` | `[[context.fields]]` with `name = "round"` | Direct field, zero-initialized in `on_init` |

All mutable state lives as direct fields on the context struct.

### Action Declarations

Actions are declared in `[[context.actions]]` entries in `blox.toml`. Each action specifies:
- `name` — action identifier used in transition/entry/exit declarations
- `kind` — `"entry"`, `"exit"`, or `"transition"` (determines closure signature)
- `fields` — list of ctx fields the action needs, with access mode (`"round:mut"`, `"self_id"`, `"peer_ref:ref"`)
- `event_payload` — optional, name of the extracted payload variable (e.g., `"do_work"`)
- `impl_required` — `true` if the function comes from an impl crate, `false` if from a context crate
- `crate` — the crate providing the function (for context crate actions)
- `fn_name` — optional, the actual function name if different from `name`

### Example

```rust
// Generated ctx.rs — plain fields, no accessor traits
pub struct PingCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    pub current_timer: Option<TimerId>,
    pub round: u32,
}
```

1. **`bloxide-core` is `no_std`** — zero OS, Tokio, or Embassy imports. `futures-core` is the only always-on runtime library dep; optional instrumentation deps (such as feature-gated `tracing`) must remain `no_std` compatible. Proc-macro crates (e.g., `bloxide-macros`) are exempt — they compile for the host and have no `no_std` impact.
2. **Blox crates are runtime-agnostic** — generic over `R: BloxRuntime`. Never import `bloxide-embassy` or any executor from a blox crate.
3. **No runtime types in messages** — domain message enums contain plain data only; no `ActorRef`, no raw senders/receivers.
4. **Shared messages in dedicated crates** — message types used by two or more blox crates live in a `*-messages` crate to avoid circular dependencies.
5. **Only leaf states as transition targets** — the engine `debug_assert`s this; violating it in release is UB.
6. **`on_entry` / `on_exit` are infallible** — they are `fn(&mut Ctx)` with no `Result`. Fallible work belongs in a `TransitionRule`'s `actions` function or is deferred to the target state's `on_entry`.
7. **Actions before guards** — event handlers use `TransitionRule { matches, actions, guard }`. All side effects go in `actions: fn(&mut Ctx, &Event)`. Guards are pure: `guard: fn(&Ctx, &ActionResults, &Event) -> Guard`. The borrow checker enforces this — `guard` receives `&Ctx` and `&ActionResults`, not `&mut Ctx`.
8. **Bubbling is implicit** — states with no matching rule automatically bubble to the parent. Empty `transitions: &[]` means all events bubble. Never add a catch-all rule that manually returns a parent; bubbling happens automatically when no rule matches.
9. **Blox crates never import impl crates** — concrete types are only referenced by the binary. Blox crates depend on context crates (free action functions) only.
10. **Context crates are portable interface layers** — context crates (`blox-ctx-*`, `bloxide-messaging`, `bloxide-peers`, etc.) define free action functions taking concrete params. They may contain portable generic action logic, but no runtime-specific imports, file I/O, or Embassy/Tokio code.
11. **Use named struct variants in message enums** — `PingPongMsg::Ping(Ping { round })` not `PingPongMsg::Ping(u32)`. Named fields are accessible by name across module boundaries without positional fragility.
12. **Lifecycle commands flow through dispatch() at VirtualRoot level** — actors handle them as domain events via `root_transitions()`. The VirtualRoot intercepts `LifecycleCommand` variants (Start, Reset, Stop, Ping) before they reach user-declared states. `Start` exits Init and enters `initial_state()`; `Reset` goes to user-defined `initial_state()` (actor immediately operational); `Stop` goes to Init (suspended, can be restarted with `Start`). Actors never implement `is_start()` or call `machine.start()`/`machine.reset()` explicitly — lifecycle is driven entirely by dispatch. `root_transitions()` returns `&[]` for supervised actors (lifecycle handled by VirtualRoot, not user code). Actors can also self-stop via `Guard::Stop` (goes to Init, reports `Stopped` to supervisor) or self-restart via `Guard::Reset`.
13. **`is_error` takes precedence** — if a state returns `true` for `is_error()`, the runtime reports `ChildLifecycleEvent::Failed`. There is no `is_terminal()` — actors self-suspend via `Guard::Stop` (returns `DispatchOutcome::Stopped`) instead of reaching terminal states. Use `is_error` for fault states that should trigger supervisor intervention; use `Guard::Stop` for normal completion (the actor self-suspends to Init and the supervisor decides next steps via `ChildPolicy`).
14. **No accessor traits on context structs** — context structs are plain structs with plain fields. Action functions take concrete params extracted from the context fields. The codegen emits wrapper closures that extract fields and call the action functions. There is no `B` generic, no `behavior: B` field, no `#[derive(BloxCtx)]`, no `#[provides]`, no `#[delegatable]`, no `#[delegates]`.

15. **Logging ripped out of blox crates** — all `bloxide-log` usage has been removed from blox crates. The `bloxide-log` crate stays in place for runtime/context crate usage. Domain-level logging re-design is a deferred decision. Never add `blox_log_*!` calls to blox crates or add `bloxide-log` as a dependency of a blox crate.

16. **Dynamic actor spawning via factory injection** — Blox crates never declare `R: SpawnCap`. Dynamic spawning uses factory injection via constructor fields in blox context structs (auto-detected by naming convention, e.g. `foo_factory: fn(...) -> ...`). The binary (or impl crate) provides the concrete factory closure at construction time. This keeps blox crates portable across all runtimes, including Embassy which lacks `SpawnCap`.
17. **KillCapability is a runtime capability, not a message** — `KillCapability::kill(handle)` immediately aborts the child's task without any callbacks firing. No `on_exit` handlers run; the task is dropped in-place. KillCapability is for (1) unresponsive actors that cannot process Stop, or (2) cleanup of stopped actors whose resources should be freed immediately. Kill works for both static and dynamic actors; killed actors are permanently dead and cannot be restarted — normal lifecycle uses Reset/Stop through dispatch(). KillCapability lives in `bloxide-core` as a trait; runtimes implement it (`NoKill` for Embassy, `Kill` for Tokio via `R::abort`). Supervisors store the concrete `TaskHandle` per child; actors never see it.

## Development Workflow

1. **Spec first** — Write/update `spec/bloxes/<name>.md` with state diagram, events, transitions
2. **Generate** — Run `cargo blox generate` to regenerate boilerplate from `blox.toml` specs
3. **Tests next** — Write `TestRuntime`-based tests per acceptance criteria
4. **Then code** — Implement `MachineSpec` to pass tests
5. **Review** — Verify impl matches spec; update tests if gaps found
6. **Keep in sync** — Update spec diagrams if implementation reveals spec errors

See `skills/building-with-bloxide/SKILL.md` for the full step-by-step workflow.

## Clarification: Factory Injection and Supervision

If you're confused about constructor fields or why supervised actors return `&[]` for `root_transitions()`, read `spec/architecture/13-factory-injection-and-supervision.md`. It contains:

- **Layer-by-layer walkthrough** of factory injection for dynamic spawning
- **How constructor fields work** and what code they generate
- **Why lifecycle events bypass the actor's handler table** (two-stream architecture)
- **Decision trees** for choosing context fields
