# Agent Orientation: Bloxide

Read this file first whenever you start a session on this repository.

> **NO BACKWARDS COMPATIBILITY.** This project does not preserve backwards
> compatibility — not for APIs, CLIs, config formats, terminology, or
> architecture layers. When something changes, update every call site, every
> doc, and every fixture in the same change, and delete the old form
> completely. Never add legacy aliases, deprecation periods, compatibility
> shims, feature bridges, or "kept for backwards compatibility" notes.

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
  bloxes/                      ← pure-TOML blox sources: each holds ONLY blox.toml + tests/<name>.rs (integration tests)
    ping/                      ← declarative Ping actor; depends on context crates (blox-ctx-rounds, blox-ctx-ping-pong) and bloxide-timer
    pong/                      ← declarative Pong actor; depends on context crates (blox-ctx-ping-pong)
    worker/                    ← declarative Worker actor; depends on context crates (blox-ctx-pool-ref, bloxide-peers)
    pool/                      ← declarative Pool actor; depends on context crates (blox-ctx-pool-ref, bloxide-peers)
    counter/                   ← declarative Counter actor; depends on context crates (blox-ctx-ticks)
    bhsm-tst/                  ← declarative Miro Samek HSM test blox (states S/S1/S11/S2/S21/S211); depends on context crate (blox-ctx-noop)
  crates/
    bloxide-core/              ← HSM engine, BloxRuntime trait, channel traits, messaging data types (ActorRef, Envelope, ActorId) (no_std)
    bloxide-log/               ← feature-gated logging macros (log / defmt backends); no_std
    bloxide-macros/            ← proc macros: channels!, dyn_channels!, next_actor_id!
    bloxide-timer/             ← timer feature crate: TimerCommand, TimerId, TimerQueue, TimerService trait, set_timer/cancel_timer/cancel_timer_by_id action functions
    bloxide-child-management/  ← child-management feature crate: ChildGroup, ChildPolicy, ChildCtrl/RegisterChild/RegisterDynamicChild control plane, supervision action functions
    bloxide-supervisor/        ← supervisor blox (reference consumer): blox.toml + generated + concrete_spec.rs (test fixture) + tests only (spec 18)
    bloxide-peers/             ← peer introduction: PeerCtrl<M,R>, AddPeer, RemovePeer, introduce_peers, broadcast_to_peers (domain-agnostic, M: Clone)
    bloxide-spawn/             ← spawn capability: ActorParts + spawn_actor_task (platform spawn), SpawnFn, SpawnOutput, SpawnCap, ChildCtrlRegistrar, spawn_dynamic_child helper (platform primitive)
    messages/
      ping-pong-messages/      ← PingPongMsg shared by both ping and pong bloxes
      pool-messages/           ← PoolMsg, WorkerMsg, DoWork, WorkDone, etc. shared by pool and worker
      counter-messages/        ← CounterMsg shared by counter blox and minimal wiring demo
      bhsm-tst-messages/       ← BhsmTstMsg shared by the bhsm-tst HSM topology demo
    context/
      blox-ctx-noop/           ← shared noop action function for demo/test bloxes (composable context-crate example; first consumer: bhsm-tst)
      blox-ctx-ping-pong/      ← ping/pong demo context crate: send_ping/send_pong/send_initial_ping/schedule_resume action functions
      blox-ctx-pool-ref/       ← domain context crate + notify_pool_done action function
      blox-ctx-rounds/         ← increment_round action function
      blox-ctx-ticks/          ← increment_count action function
    impl/
      tokio-pool-demo-impl/    ← impl crate: free functions (process_work, build_worker, pool action handlers) for pool demo (standalone committed crate, NOT a root workspace member — root `exclude` entry; built as a path dep of the generated example crates; tests run via the `cargo blox test` impl-crate step)
    tools/
      bloxide-codegen/         ← TOML-driven code generator library
      cargo-blox/              ← CLI: cargo blox generate / new / build / check / test / run
  tools/
    bloxide-viz-export/        ← source-to-JSON exporter for visualizer
    bloxide-visualizer/        ← browser-based state-machine visualizer
  runtimes/
    bloxide-embassy/           ← Embassy runtime implementation
    bloxide-tokio/             ← Tokio runtime implementation; implements SpawnCap and DynamicChannelCap
    bloxide-test-runtime/      ← test runtime for executor-free unit testing (no async executor; implements DynamicChannelCap + SpawnCap — kill is a documented no-op)
  examples/                    ← pure-TOML wiring sources: each holds system.toml (+ tests/ for tokio-pool-demo)
    embassy-demo/              ← system.toml: ping/pong on Embassy (host-runnable)
    tokio-demo/                ← system.toml: ping/pong on Tokio
    tokio-minimal-demo/        ← system.toml: counter actor on Tokio
    tokio-pool-demo/           ← system.toml + tests/: pool/worker on Tokio
  scripts/                     ← CI helper scripts (ci.sh)
  AGENTS.md                    ← this file
```

`cargo blox generate` materializes a complete cargo workspace at
`target/bloxide-generated/` (build artifact, gitignored): members `crates/*`
(the six generated blox crates — Cargo.toml, build.rs, src/lib.rs with prelude
re-exports + `[[consts]]`, src/generated/*, mirrored tests/) and `examples/*`
(the four generated example crates). Each generated crate has a build.rs that
re-syncs from the source blox.toml/system.toml, so plain cargo commands work
inside `target/bloxide-generated/` after one `cargo blox generate`. Generated
crate name = kebab actor name + `-blox` (Ping → `ping-blox`). `generate` also
writes `.vscode/settings.json` (gitignored) with
`rust-analyzer.linkedProjects = ["./Cargo.toml", "./target/bloxide-generated/Cargo.toml"]`,
so IDE indexing covers both workspaces.

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

This is the application-author view described in `skills/building-with-bloxide/SKILL.md` and `spec/architecture/11-action-crate-pattern.md`.

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
- Tier 2: runtime-facing capabilities such as `StaticChannelCap`, `DynamicChannelCap`, `GroupChannelCap`, `TimerService`, `SpawnCap`, `KillCapability` (the actor run loop is the unified `run()` + `RunConfig` in `bloxide-core`, not a trait)

With the elimination of accessor traits and the `B` generic,
context structs are plain structs with plain fields. Action functions
take concrete params extracted from the context fields. The codegen
emits wrapper closures that extract fields and call the action functions.

Use this model when you are wiring runtimes, reading macro output, or adding new framework capabilities.


## Where to Find Things

### Suggested Reading Order

1. **README.md** — repo map and runnable examples
2. **AGENTS.md** (this file) — mental models, key invariants, where-to-find-things table
3. **skills/building-with-bloxide/SKILL.md** — end-to-end build workflow
4. **QUICK_REFERENCE.md** — decision trees and lookup tables when you're stuck

Then dive deeper as needed:
- `spec/architecture/01-hsm-engine.md` — `MachineSpec`, dispatch, and the five-level lifecycle (reset → stop → done → abort → kill)
- `spec/architecture/04-handler-patterns.md` — transition patterns and declarative TOML transitions
- `spec/architecture/07-supervision.md` — supervisor patterns
- `spec/architecture/10-dynamic-actors.md` — dynamic spawning and factory injection

| Question | File |
|---|---|
| What is the layered architecture and two-tier trait system? | `spec/architecture/00-layered-architecture.md` |
| How does the overall system fit together? | `spec/architecture/00-layered-architecture.md` (System Overview) |
| How do HSMs, the engine, and the lifecycle model work? | `spec/architecture/01-hsm-engine.md` (HSM Engine & Lifecycle) |
| How do actors send messages? | `spec/architecture/02-actor-messaging.md` |
| How are actors wired at startup? | `spec/architecture/03-static-wiring.md` |
| What are the named handler and topology patterns? | `spec/architecture/04-handler-patterns.md` |
| How do actions, logging, and declarative TOML transitions work? | `spec/architecture/05-actions.md` |
| How do typed mailboxes and priority ordering work? | `spec/architecture/06-typed-mailboxes.md` |
| How does supervision work? | `spec/architecture/07-supervision.md` |
| How is an application wired end to end? | `spec/architecture/08-application.md` |
| How do effects (timers) and capabilities work? | `spec/architecture/09-effects-and-capabilities.md` |
| **How do context crates, impl crates, and bloxes fit together?** | **`spec/architecture/11-action-crate-pattern.md`** |
| How do dynamic actors, factory injection, and peer introduction work? | `spec/architecture/10-dynamic-actors.md` |
| How does factory injection interact with supervision? | `spec/architecture/12-factory-injection-and-supervision.md` |
| How do composable context crates work? | `spec/architecture/13-composable-context-crates.md` |
| How does declarative wiring and handle injection work? | `spec/architecture/14-declarative-wiring.md` |
| How does blox.toml serve as the source of truth? | `spec/architecture/15-blox-toml-source-of-truth.md` |
| How does spawning work? | `spec/architecture/16-spawn-architecture.md` |
| How does the cargo-blox CLI work? What commands exist? | `spec/architecture/17-cli-design.md` |
| Spec for the Ping actor | `spec/bloxes/ping.md` |
| Spec for the Pong actor | `spec/bloxes/pong.md` |
| Spec for the Counter actor | `spec/bloxes/counter.md` |
| Spec for the Pool actor | `spec/bloxes/pool.md` |
| Spec for the Worker actor | `spec/bloxes/worker.md` |
| Spec for the BHSM test actor | `spec/bloxes/bhsm-tst.md` |
| How does the reusable supervisor spec work? | `spec/architecture/07-supervision.md` |
| How are platform features packaged and consumed? | `spec/architecture/18-platform-feature-pattern.md` |
| Template for a new blox | `spec/templates/blox-spec.md` |
| **How do I test a blox in isolation?** | `runtimes/bloxide-test-runtime/src/runtime.rs` |
| **What is TestRuntime for?** | `runtimes/bloxide-test-runtime/src/runtime.rs` |
| **How do I test timers without an executor?** | `crates/bloxide-timer/src/test_utils.rs` (`VirtualClock`) — crate-internal (`#[cfg(test)]`); the reference pattern for driving a `TimerQueue` with a virtual clock in crate tests |
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
`role` is validated: `ctor` (constructor parameter) and `state` (zero-initialized field) are the
only values.

| Field | Source | Generates |
|-------|--------|-----------|
| `self_id: ActorId` | Auto-emitted by codegen (always) | Plain field |
| `foo_ref: ActorRef<M, R>` | `[[context.uses]]` with `field = "foo_ref"` | Plain field |
| `foo_factory: fn(...) -> ...` | `[[context.uses]]` with `role = "ctor"` | Constructor parameter |
| `round: u32` | `[[context.fields]]` with `name = "round"` | Direct field, zero-initialized in `on_init` |

All mutable state lives as direct fields on the context struct.

### Crate Packaging: `[package]` and `[[consts]]`

Blox sources are pure TOML — `bloxes/<name>/` contains only `blox.toml` and
`tests/<name>.rs`; the cargo crate is materialized into
`target/bloxide-generated/crates/<name>-blox/` by `cargo blox generate`.
Packaging for that crate is declared in `blox.toml`:

- `[package]` — `description` for the generated Cargo.toml.
- `[package.features]` — feature flags. Regular dependencies are derived from
  the message paths, context imports, and action crates declared below; only
  features need declaring here. E.g. ping: `default = ["std"]`,
  `std = ["bloxide-core/std", "bloxide-timer/std"]`; pool also declares
  `dynamic = []` and its default is `["std", "dynamic"]`.
- `[package.dependencies]` / `[package.dev-dependencies]` — explicit deps when
  derivation isn't enough. Dev-dependencies cover everything
  `tests/<name>.rs` imports (integration tests do not see the crate's regular
  dependencies — declare everything they import).
- `[[consts]]` — `name`, `ty`, `value`, optional `doc`; emitted at the
  generated crate root (e.g. ping's `MAX_ROUNDS`/`PAUSE_AT_ROUND`, counter's
  `DONE_AT_COUNT`). Guards reference them via `spec_imports` —
  `spec_imports = ["crate::MAX_ROUNDS"]` keeps working.

### Action Declarations

Actions are declared in `[[context.actions]]` entries in `blox.toml`. There is no `kind` key — the use site (transition vs entry/exit slot) determines the closure signature the codegen generates; a stale `kind` key is a hard parse error. Each action specifies:
- `name` — action identifier used in transition/entry/exit declarations
- `fields` — list of ctx fields the action needs, with access mode (`"round:mut"`, `"self_id"`, `"peer_ref:ref"`)
- `event_payload` — optional, name of the extracted payload variable (e.g., `"do_work"`)
- `impl_required` — `true` if the function comes from an impl crate, `false` if from a context crate
- `crate` — the crate providing the function; required for context crate actions (omitted when `impl_required = true`)
- `fn_name` — optional, the actual function name if different from `name`
- `returns` — optional; the only recognized value is `"ActionResult"`. When set, the transition wrapper emits the call bare (no `ActionResult::from(...)` normalization — it would be a same-type conversion flagged by clippy). Any other value is a hard error.

### Example

```rust
// Generated ctx (materialized crate target/bloxide-generated/crates/ping-blox/src/generated/)
// — plain fields, no accessor traits
pub struct PingCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    pub current_timer: Option<TimerId>,
    pub round: u32,
}
```

## Key Invariants

These must never be violated. Violating them silently breaks the architecture.
Other docs (QUICK_REFERENCE.md, spec/README.md, the skills) link here instead
of restating the list — this is the single canonical copy.

1. **`bloxide-core` is `no_std`** — zero OS, Tokio, or Embassy imports. `futures-core` is the only always-on runtime library dep; optional instrumentation deps (such as feature-gated `tracing`) must remain `no_std` compatible. Proc-macro crates (e.g., `bloxide-macros`) are exempt — they compile for the host and have no `no_std` impact.
2. **Blox crates are runtime-agnostic** — generic over `R: BloxRuntime`. Never import `bloxide-embassy` or any executor from a blox crate.
3. **No runtime types in messages** — domain message enums contain plain data only; no `ActorRef`, no raw senders/receivers.
4. **Shared messages in dedicated crates** — message types used by two or more blox crates live in a `*-messages` crate to avoid circular dependencies.
5. **Only leaf states as transition targets** — the engine `debug_assert`s this; violating it in release is UB.
6. **`on_entry` / `on_exit` are infallible** — they are `fn(&mut Ctx)` with no `Result`. Fallible work belongs in a `TransitionRule`'s `actions` function or is deferred to the target state's `on_entry`.
7. **Actions before guards** — event handlers use `TransitionRule { matches, actions, guard }`. All side effects go in `actions: fn(&mut Ctx, &Event) -> ActionResult`. Guards are pure: `guard: fn(&Ctx, &ActionResults, &Event) -> Decision<S>`. The borrow checker enforces this — `guard` receives `&Ctx` and `&ActionResults`, not `&mut Ctx`.
8. **Bubbling is implicit** — states with no matching rule automatically bubble to the parent. Empty `transitions: &[]` means all events bubble. Never add a catch-all rule that manually returns a parent; bubbling happens automatically when no rule matches.
9. **Blox crates never import impl crates** — concrete types are only referenced by the binary. Blox crates depend on context crates (free action functions) only.
10. **Context crates are portable interface layers** — context crates (`blox-ctx-*`, `bloxide-peers`, `bloxide-timer`, etc.) define free action functions taking concrete params. They may contain portable generic action logic, but no runtime-specific imports, file I/O, or Embassy/Tokio code.
11. **Use named struct variants in message enums** — `PingPongMsg::Ping(Ping { round })` not `PingPongMsg::Ping(u32)`. Named fields are accessible by name across module boundaries without positional fragility.
12. **Lifecycle commands flow through dispatch() and are intercepted by the VirtualRoot engine** before they reach user-declared states — actors never implement `is_start()` or call `machine.start()`/`machine.reset()` explicitly; lifecycle is driven entirely by dispatch. (The VirtualRoot interception is engine behavior, distinct from `root_transitions()` — see below.) The lifecycle model is five levels, in increasing severity: **reset → stop → done → abort → kill**. `Start` exits Init and enters `initial_state()`; `Reset` goes to user-defined `initial_state()` (actor immediately operational); `Stop` goes to Init (suspended, can be restarted with `Start`). Actors can also self-stop via `Decision::Stop` (goes to Init, reports `Stopped` to supervisor), self-terminate cleanly via `Decision::Done` (exit chain + `on_init_entry`, then the task ends, reports `Done`), or self-restart via `Decision::Reset`. `root_transitions()` is the separate VirtualRoot fallback for **domain** events that bubble past all user states; root rules are ordinary `[[topology.transitions]]` entries with the reserved keyword `state = "root"` (`"root"` cannot name a user state) and are emitted by the codegen — supervised actors typically declare none (lifecycle is handled by the VirtualRoot engine, not user code).
13. **`is_error` takes precedence** — if a state returns `true` for `is_error()`, the runtime reports `ChildLifecycleEvent::Failed`. There is no `is_terminal()` — actors end themselves via decision outcomes, not terminal states. Use `is_error` for fault states that should trigger supervisor intervention; use `Decision::Done` for normal completion (the task ends and the supervisor deregisters the child — no restart policy); use `Decision::Stop` only for suspend/resume (the actor waits in Init and the supervisor decides next steps via `ChildPolicy`). Supervised tasks **stay alive** on `Failed` (`RunConfig.exit_on_fail = false` for supervised actors): the actor parks in its absorbing error state and the supervisor's `ChildPolicy` applies (`Reset` revives it). Root/unsupervised/bare actors set `exit_on_fail = true` — `Failed` ends the task.
14. **No accessor traits on context structs** — context structs are plain structs with plain fields. Action functions take concrete params extracted from the context fields. The codegen emits wrapper closures that extract fields and call the action functions. There is no `B` generic, no `behavior: B` field, no `#[derive(BloxCtx)]`, no `#[provides]`, no `#[delegatable]`, no `#[delegates]`.

15. **Logging ripped out of blox crates** — all `bloxide-log` usage has been removed from blox crates. The `bloxide-log` crate stays in place for runtime/context crate usage. Domain-level logging re-design is a deferred decision. Never add `blox_log_*!` calls to blox crates or add `bloxide-log` as a dependency of a blox crate.

16. **Dynamic actor spawning via factory injection** — Blox crates never declare `R: SpawnCap`. Dynamic spawning uses factory injection via constructor fields in blox context structs (declared as `[[context.uses]]` entries with `role = "ctor"`, e.g. `spawn_fn: SpawnFn<R, Req>`). The binary (or impl crate) provides the concrete factory function at construction time. Impl-crate factories do pure construction and return `bloxide_spawn::ActorParts` (no `run()`, `RunConfig`, or `SpawnCap`); the platform helper `bloxide_spawn::spawn_actor_task` — composed with the factory at the wiring site by the system codegen — owns `RunConfig`, task spawn, and kill-handle derivation. This keeps blox crates portable across all runtimes, including Embassy which lacks `SpawnCap`.
17. **KillCapability is a runtime capability, not a message** — `KillCapability::kill(handle)` immediately aborts the child's task without any callbacks firing. No `on_exit` handlers run; the task is dropped in-place. KillCapability is for (1) unresponsive actors that cannot process Stop, or (2) cleanup of stopped actors whose resources should be freed immediately. Kill works for both static and dynamic actors; killed actors are permanently dead and cannot be restarted — normal lifecycle uses Reset/Stop through dispatch(). `KillCapability` lives in `bloxide-core` as a trait (with `NoKill` for static runtimes); the `Kill` type lives in `bloxide-spawn` because it requires the `SpawnCap` bound. Each runtime picks one via the `BloxRuntime::Kill` associated type (`NoKill` for Embassy, `Kill` for Tokio and TestRuntime). The trait carries a `CAN_KILL` associated const (`false` for `NoKill`, `true` for `Kill`): `ChildPolicy::Kill` is refused at registration on `!CAN_KILL` runtimes — a no-op kill must never let a supervisor mark a live child dead. TestRuntime's kill is **recorded, not executed** (`drain_killed()` / `kill_count()`), so kill paths are exercisable in unit tests. Supervisors store the concrete `KillHandle` per child (cloneable, so action functions can extract it from `&Event`); actors never see it.
18. **System.toml is the single source of truth for concrete action wiring** — Blox-crate-level codegen ALWAYS produces stub `spec_skeletons`. ("Blox-crate level" means the materialized crate under `target/bloxide-generated/crates/` — there is no committed blox crate; blox integration tests run from the materialized crate.) The system-level codegen (from `system.toml`) ALWAYS produces concrete action closures, for every actor including dynamically spawned ones. There is no `crate = "crate"` path at the blox-crate level. The action contract is uniform: a transition action function returns `ActionResult`, `Result<(), E>`, or `()` — the codegen wrapper normalizes via `ActionResult::from(...)`; declare `returns = "ActionResult"` on the `[[context.actions]]` entry to skip the wrapper when the function already returns `ActionResult` (any other `returns` value is a hard error). Entry/exit functions are infallible `fn(&mut Ctx)` (any return value is discarded). Dynamic actors are declared in `system.toml` with `kind = "dynamic"` — they get a concrete spec generated but no channels/tasks/bootstrap in main.rs. The impl crate's spawn function is generic over the spec type; the generated main.rs monomorphizes it with the system-level concrete spec. Supervision strategy vocabulary in `system.toml` is `when_any_done` / `when_all_done` (maps to `GroupShutdown` variants; unknown values are hard errors).
19. **Platform features are consumed uniformly** (spec 18) — a platform feature crate owns its message set, context field declarations, and consumer-side action functions (concrete params or extracted payloads, never the consumer's event enum). Blox crates compose features via `[[context.uses]]` / `[[context.actions]]` and never own feature logic or feature message sets. The supervisor is the reference consumer: topology only, everything else from `bloxide-child-management` + `bloxide-spawn`. Factory/capability features (spawn) are the documented exception to message-set ownership.
20. **The supervision control plane is confirm-before-record** — `ChildGroup` phase transitions happen only on confirmed delivery or observed lifecycle reports, never on attempted sends. `try_send` error kinds are load-bearing: `Closed` proves the child's task is gone (terminal `Gone` phase, shutdown evaluated); `Full` queues the command in the per-child `pending_cmd` slot, retried by `flush_pending` on every event pass. Registration is fallible (`try_add`/`try_add_dynamic` → `RegistrationError`, warn-and-drop) — never a panic on message data. `ShuttingDown` records every terminal signal (`Stopped`/`Done`/`Failed`/`Aborted`/`Killed`/`Gone`) and waits forever by design (no timeout — an unresponsive child cannot be killed on static runtimes anyway). Reporting-side semantics: `Ping` is answered with `Alive` only from operational, non-error states; redundant `Start` is acknowledged with `Started` (mirroring `Stop`-in-Init); `Started(error)` is normalized to `Failed` at the source; a run loop whose domain streams ALL close while operational (all-streams-close — the actor became unreachable while the supervisor lives) reports `Failed` before exiting, while lifecycle/abort stream closure and all-streams-close in Init are the expected supervisor-teardown cascade and stay silent. Health checks require `MAX_MISSES` (2) consecutive unanswered delivered Pings before declaring a child rogue; an undelivered Ping is never a miss.

## Development Workflow

0. **Bootstrap (fresh clone)** — Install the CLI (`cargo install --path crates/tools/cargo-blox`), then run `cargo blox generate`. This is mandatory after checkout: it materializes `target/bloxide-generated/` and writes `.vscode/settings.json`, so both cargo and IDE indexing work.
1. **Spec first** — Write/update `spec/bloxes/<name>.md` with state diagram, events, transitions
2. **Generate** — Run `cargo blox generate` to (re)materialize the generated workspace from `bloxes/*/blox.toml` and `examples/*/system.toml`
3. **Tests next** — Write `TestRuntime`-based integration tests in `bloxes/<name>/tests/<name>.rs` per acceptance criteria
4. **Then code** — Implement `MachineSpec` to pass tests
5. **Review** — Verify impl matches spec; update tests if gaps found
6. **Keep in sync** — Update spec diagrams if implementation reveals spec errors

`cargo blox build` / `check` / `test` with no flags run BOTH the repo workspace
and the generated workspace (`--example <name>` scopes to one example).
`cargo blox run --example <name> [-- args]` runs an example; without
`--example` it errors and lists the available examples.

See `skills/building-with-bloxide/SKILL.md` for the full step-by-step workflow.

## Clarification: Factory Injection and Supervision

If you're confused about constructor fields or why supervised actors return `&[]` for `root_transitions()`, read `spec/architecture/12-factory-injection-and-supervision.md`. It contains:

- **Layer-by-layer walkthrough** of factory injection for dynamic spawning
- **How constructor fields work** and what code they generate
- **Why lifecycle events bypass the actor's handler table** (two-stream architecture)
- **Decision trees** for choosing context fields
