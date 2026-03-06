# Agent Orientation: Bloxide

Read this file first whenever you start a session on this repository.

## What This Project Is

Bloxide is a `no_std` hierarchical state machine (HSM) + actor messaging framework written in Rust, targeting Embassy (embedded) as its first runtime. It provides a `MachineSpec` trait that domain code implements to define state topologies, event handlers, and context — all without importing any runtime. A separate runtime crate (currently `bloxide-embassy`) wires actors together and runs them on an executor.

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
  runtimes/
    bloxide-embassy/           ← Embassy runtime implementation
    bloxide-tokio/             ← Tokio runtime implementation; implements SpawnCap and DynamicChannelCap
  examples/
    messages/
      ping-pong-messages/      ← PingPongMsg shared by both ping and pong bloxes
      pool-messages/           ← PoolMsg, WorkerMsg, DoWork, WorkDone, etc. shared by pool and worker
    actions/
      ping-pong-actions/       ← HasPeerRef, CountsRounds, send_initial_ping, send_pong, etc. (no concrete types)
      pool-actions/            ← WorkerSpawnFn, HasWorkers, HasWorkerFactory, HasCurrentTask, introduce_new_worker, etc.
    embassy-demo-impl/         ← impl crate: PingBehavior (concrete behavior for Ping)
    bloxes/
      ping/                    ← declarative Ping actor; depends on ping-pong-actions
      pong/                    ← declarative Pong actor; depends on ping-pong-actions
      worker/                  ← declarative Worker actor; depends on pool-actions (no pool-blox dependency)
      pool/                    ← declarative Pool actor; depends on pool-actions (no worker-blox dependency)
    embassy-demo/              ← binary: wires ping/pong actors and spawns Embassy tasks
    tokio-demo/                ← binary: wires ping/pong actors on Tokio
    tokio-pool-demo/           ← binary: wires pool/worker actors; provides spawn_worker factory
  AGENTS.md                    ← this file
```

## Where to Find Things

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
| **How do action crates, impl crates, and bloxes fit together?** | **`spec/architecture/10-action-crate-pattern.md`** |
| How do dynamic actors, factory injection, and peer introduction work? | `spec/architecture/11-dynamic-actors.md` |
| Spec for the Ping actor | `spec/bloxes/ping.md` |
| Spec for the Pong actor | `spec/bloxes/pong.md` |
| Spec for the Supervisor blox | `spec/architecture/08-supervision.md` |
| Template for a new blox | `spec/templates/blox-spec.md` |

## Skills

Skills are reusable workflows. Read the relevant skill file before starting the corresponding task.

| Task | Skill |
|---|---|
| Building bloxes (new or modified) | `skills/building-with-bloxide/SKILL.md` |
| Evolving the framework (engine, runtimes, stdlib crates, macros) | `skills/contributing-to-bloxide/SKILL.md` |

The building guide is portable — downstream projects that depend on bloxide should copy `skills/building-with-bloxide/` into their repo and reference it from their own AGENTS.md.

## Key Invariants — Never Violate These

1. **`bloxide-core` is `no_std`** — zero OS, Tokio, or Embassy imports. Only `futures-core` is allowed as a runtime library dep. Proc-macro crates (e.g., `bloxide-macros`) are exempt — they compile for the host and have no `no_std` impact.
2. **Blox crates are runtime-agnostic** — generic over `R: BloxRuntime`. Never import `bloxide-embassy` or any executor from a blox crate.
3. **No runtime types in messages** — domain message enums contain plain data only; no `ActorRef`, no raw senders/receivers.
4. **Shared messages in dedicated crates** — message types used by two or more blox crates live in a `*-messages` crate to avoid circular dependencies.
5. **Only leaf states as transition targets** — the engine `debug_assert`s this; violating it in release is UB.
6. **`on_entry` / `on_exit` are infallible** — they are `fn(&mut Ctx)` with no `Result`. Fallible work belongs in a `TransitionRule`'s `actions` function or is deferred to the target state's `on_entry`.
7. **Actions before guards** — event handlers use `TransitionRule { matches, actions, guard }`. All side effects go in `actions: fn(&mut Ctx, &Event)`. Guards are pure: `guard: fn(&Ctx, &ActionResults, &Event) -> Guard`. The borrow checker enforces this — `guard` receives `&Ctx` and `&ActionResults`, not `&mut Ctx`.
8. **Bubbling is implicit** — states with no matching rule automatically bubble to the parent. Empty `transitions: &[]` means all events bubble. Never add a "catch-all" rule that manually returns `Parent`.
9. **Blox crates never import impl crates** — concrete types are only referenced by the binary. Blox crates depend on actions crates (traits + generic functions) only.
10. **Action crates are interface-only** — action crates (`*-actions`) define traits and trait-bounded generic functions. They contain zero concrete logic (no `tracing`, no file I/O, no Embassy).
11. **Use named struct variants in message enums** — `PingPongMsg::Ping(Ping { round })` not `PingPongMsg::Ping(u32)`. Named fields are accessible by name across module boundaries without positional fragility.
12. **Actors never handle lifecycle events** — the runtime manages Start, Terminate, Stop, and Ping via `machine.start()` and `machine.reset()` directly. Actors can be restarted (Terminate keeps the task alive in Init) or permanently stopped (Stop exits the task). No `LifecycleMsg`, `LifecycleStatusMsg`, `HasSupervisorRef`, or `supervisor_ref` fields in blox crates. `root_transitions()` returns `&[]` for supervised actors.
13. **`is_error` takes precedence over `is_terminal`** — if a state returns `true` for both `is_error()` and `is_terminal()`, the runtime reports only `ChildLifecycleEvent::Failed` (not `Done`). Use `is_error` for fault states that should trigger supervisor intervention, and `is_terminal` for normal completion.
14. **Logging via `bloxide-log` macros** — use `blox_log_info!`, `blox_log_debug!`, etc. from `bloxide-log`. Logging is a compile-time feature flag (`log` or `defmt`), not a runtime trait. Never add a `Logs` trait or logger generic parameter to blox contexts.

## Development Workflow

```
spec first  →  write / update spec/bloxes/<name>.md
tests next  →  write TestRuntime-based tests per acceptance criteria
then code   →  implement MachineSpec to pass tests
keep in sync →  update spec diagrams if implementation reveals spec errors
```

See `skills/building-with-bloxide/SKILL.md` for the full step-by-step workflow.
