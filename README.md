# Bloxide

**Hierarchical state machine actors for Rust — runtime-agnostic, from Embassy bare-metal to Tokio.**

[![CI](https://github.com/bloxide-com/bloxide/actions/workflows/lint-and-test.yml/badge.svg)](https://github.com/bloxide-com/bloxide/actions/workflows/lint-and-test.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 2021](https://img.shields.io/badge/rust-2021_edition-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2021/)

Bloxide is a hierarchical state machine (HSM) + actor messaging framework. Domain actors ("bloxes") are generic over `BloxRuntime` so the same state machine logic runs on Embassy *and* Tokio without modification. A separate runtime crate wires channels, spawns tasks, and drives the state machine.

---

## Features

- **Hierarchical state machines** — composite states, event bubbling, entry/exit callbacks, run-to-completion dispatch
- **Runtime-agnostic actors** — blox code depends only on `bloxide-core`; never imports a runtime
- **Built-in supervision** — reusable OTP-inspired `SupervisorSpec<R>` and `bloxide-supervisor` primitives manage child actor lifecycle out of the box
- **Tokio + Embassy runtimes** — `bloxide-tokio` and `bloxide-embassy` (`no_std`) ship ready to use; each provides async channels, supervision, and timer services wired to its executor
- **Dynamic actors** — spawn new actors at runtime with factory injection and automatic peer introduction (gated by the `dynamic` Cargo feature on `pool-blox` and `tokio-pool-demo-impl`)

---

## Start Here

- Read [AGENTS.md](AGENTS.md) for the three-layer principle, four-layer application structure, and two-tier trait system in one place.
- Use [skills/building-with-bloxide/SKILL.md](skills/building-with-bloxide/SKILL.md) as the step-by-step build workflow.
- Keep [skills/building-with-bloxide/reference.md](skills/building-with-bloxide/reference.md) open as the API reference while you build.
- For the smallest runnable example, start with `cargo blox run --example tokio-minimal-demo` (now fully four-layered via `counter-*` crates).

---

## Quick look

A blox implements `MachineSpec` to define states, transitions, and context. At startup the runtime creates channels, builds `StateMachine` instances, and spawns tasks. Here is a trimmed view of the Tokio demo wiring two supervised ping-pong actors:

```rust
// Create typed channels for each actor
let ((ping_ref,), ping_mbox) = bloxide_tokio::channels! { PingPongMsg(16) };
let ((pong_ref,), pong_mbox) = bloxide_tokio::channels! { PingPongMsg(16) };
let ping_id = ping_ref.id();
let pong_id = pong_ref.id();

// Build state machines — PingSpec and PongSpec are runtime-agnostic MachineSpec impls
let ping_ctx = PingCtx::new(ping_id, pong_ref.clone(), ping_ref.clone(), timer_ref);
let pong_ctx = PongCtx::new(pong_id, ping_ref.clone());
let ping_machine = StateMachine::new(ping_ctx);
let pong_machine = StateMachine::new(pong_ctx);

// Define task wrappers (typically in a prelude or near main)
bloxide_tokio::actor_task_supervised!(ping_task, PingSpec<TokioRuntime>);
bloxide_tokio::actor_task_supervised!(pong_task, PongSpec<TokioRuntime>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

// Supervise both actors
let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone, 2);
let sup_notify_ref = group.notify_ref();
bloxide_tokio::spawn_static_child!(group, ping_task(ping_machine, ping_mbox, ping_id), ChildPolicy::Stop);
bloxide_tokio::spawn_static_child!(group, pong_task(pong_machine, pong_mbox, pong_id), ChildPolicy::Stop);

// Build and start the supervisor
let (children, sup_notify_rx, sup_control_rx) = group.finish();
let sup_id = bloxide_tokio::next_actor_id!();
let sup_ctx = SupervisorCtx::new(sup_id, children, sup_notify_ref);
let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
sup_machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));

// Run until shutdown
supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;
```

The blox crates (`PingSpec`, `PongSpec`) are generic over `R: BloxRuntime` — the same code runs on Embassy by swapping `TokioRuntime` for `EmbassyRuntime`.

**Key insight:** Lifecycle commands (Start/Reset/Stop) flow through `dispatch()`, not through direct `start()` calls. Supervised actors wait for `LifecycleCommand::Start` from the supervisor before entering their initial operational state.

---

## Repository layout

```
bloxide/
├── crates/            # framework + layered application crates
│   ├── bloxide-core/      # HSM engine, MachineSpec, BloxRuntime, KillCapability, run/RunConfig
│   ├── bloxide-log/       # feature-gated logging macros (log / defmt / no-op)
│   ├── bloxide-macros/    # proc macros: channels!, dyn_channels!, next_actor_id!
│   ├── bloxide-peers/     # peer introduction: PeerCtrl, introduce_peers, broadcast_to_peers, apply_peer_control
│   ├── bloxide-child-management/ # reusable child tracking: ChildGroup, ChildEntry, ChildPhase,
│   │                         #   ChildCtrl/RegisterChild/RegisterDynamicChild, ChildPolicy, GroupShutdown, action functions
│   ├── bloxide-supervisor/ # supervisor blox (reference consumer): blox.toml + generated + concrete_spec.rs + tests
│   ├── bloxide-spawn/     # spawn capability: SpawnCap, SpawnFn, SpawnOutput, ChildCtrlRegistrar, spawn_dynamic_child
│   ├── bloxide-timer/     # timer service: set_timer / cancel_timer / cancel_timer_by_id
│   ├── messages/          # shared message crates (ping-pong, pool, counter, bhsm-tst)
│   ├── context/           # composable context crates (blox-ctx-ping-pong, -pool-ref, -rounds, -ticks)
│   ├── impl/              # concrete behavior/factory crates for wiring demos (tokio-pool-demo-impl)
│   └── tools/             # codegen and CLI tools
│       ├── bloxide-codegen/ # TOML-driven code generator library
│       └── cargo-blox/    # CLI (see QUICK_REFERENCE.md → "cargo blox Command Reference")
├── runtimes/          # runtime implementations
│   ├── bloxide-embassy/   # Embassy runtime (embedded target)
│   ├── bloxide-tokio/     # Tokio runtime (std target)
│   └── bloxide-test-runtime/ # executor-free test runtime (TestRuntime)
├── bloxes/            # pure-TOML blox sources (blox.toml + tests): ping, pong, worker, pool, counter, bhsm-tst
├── examples/          # declarative wiring manifests (system.toml) + tests
│   ├── embassy-demo/
│   ├── tokio-demo/
│   ├── tokio-minimal-demo/
│   └── tokio-pool-demo/
├── skills/            # agent skills (workflows for building/evolving)
│   ├── building-with-bloxide/
│   └── contributing-to-bloxide/
├── tools/               # visualization utilities
│   ├── bloxide-viz-export/  # source-to-JSON exporter for visualizer
│   └── bloxide-visualizer/  # browser-based state-machine visualizer
├── scripts/           # CI helper scripts (ci.sh)
├── spec/              # architecture docs and per-blox specs
│   ├── architecture/      # numbered design docs
│   ├── bloxes/            # per-blox specs (ping, pong, pool, worker, counter, bhsm)
│   └── templates/         # blox-spec template
└── .github/workflows/ # CI: copyright, fmt, clippy, tests, rustdoc
```

---

## Running the examples

> **Getting started — generate first.** Generated artifacts (in-crate
> `src/generated/` for stdlib crates like bloxide-supervisor,
> `target/bloxide-generated/` for bloxes and examples, `.vscode/settings.json`)
> are **gitignored** — a
> fresh checkout contains only the `blox.toml` / `system.toml` sources. Run
> `cargo blox generate` (or `cargo run -p cargo-blox -- blox generate` when
> working from a source checkout of this repo — the `cargo blox` on PATH is an
> installed binary and may be stale) before the first build. `generate` runs
> the spec-to-code lint first and is idempotent, and all `cargo blox` commands
> resolve the workspace root, so they work from any subdirectory.
>
> **IDE setup.** `cargo blox generate` also emits `.vscode/settings.json`
> declaring `rust-analyzer.linkedProjects` for both the root `Cargo.toml` and
> `target/bloxide-generated/Cargo.toml`, so after the one-time generate step
> rust-analyzer indexes both workspaces — no manual editor configuration.

Each example has a `system.toml` wiring manifest and committed tests;
`cargo blox generate` materializes a runnable crate per example into
`target/bloxide-generated/examples/<name>/`.

```bash
# Minimal single-actor Tokio example (4-layer architecture)
cargo blox run --example tokio-minimal-demo

# Ping-pong with OTP supervision, timer-driven pause, and full HSM tracing
RUST_LOG=trace cargo blox run --example tokio-demo

# Worker pool with dynamic spawning
RUST_LOG=info cargo blox run --example tokio-pool-demo

# Embassy (std target, simulates embedded)
RUST_LOG=trace cargo blox run --example embassy-demo
```

---

## Building with `cargo blox`

Bloxide uses `cargo blox` for code generation. After defining schemas in `blox.toml` files:

```bash
cargo install --path crates/tools/cargo-blox
cargo blox generate   # regenerate all boilerplate from blox.toml specs
cargo blox build      # generate + cargo build
cargo blox check      # generate + cargo check
cargo blox test       # generate + cargo test
```

Message enums, event types, and state topology are declared in `blox.toml`; blox and example crates are materialized under `target/bloxide-generated/`, while stdlib crates (bloxide-supervisor, messages crates) generate in-crate into `src/generated/`. See `skills/building-with-bloxide/SKILL.md` for the full workflow.

---

## Crates

| Crate | Path | `no_std` | Purpose |
|---|---|:---:|---|
| `bloxide-core` | `crates/bloxide-core` | ✅ | HSM engine, `MachineSpec`, `BloxRuntime`, `StateMachine`, `KillCapability`, `run`/`RunConfig` |
| `bloxide-macros` | `crates/bloxide-macros` | ✅¹ | `channels!`, `dyn_channels!`, `next_actor_id!` |
| `bloxide-log` | `crates/bloxide-log` | ✅ | Feature-gated logging macros (`log` / `defmt` / no-op) |
| `bloxide-timer` | `crates/bloxide-timer` | ✅ | `TimerCommand`, `TimerQueue`, `set_timer`, `cancel_timer`, `cancel_timer_by_id`, `VirtualClock` (crate-internal, `#[cfg(test)]`) |
| `bloxide-child-management` | `crates/bloxide-child-management` | ✅ | `ChildGroup`, `ChildEntry`, `ChildPhase`, `ChildGroupBuilder`, `ChildPolicy`, `GroupShutdown`, `ChildCtrl`/`RegisterChild`/`RegisterDynamicChild`, action functions |
| `bloxide-supervisor` | `crates/bloxide-supervisor` | ✅ | Supervisor blox (reference consumer): `SupervisorSpec`, `SupervisorCtx`; `blox.toml` + generated + `concrete_spec.rs` test fixture + tests only |
| `bloxide-spawn` | `crates/bloxide-spawn` | ✅ | `SpawnCap`, `SpawnFn`, `SpawnOutput`, `ChildCtrlRegistrar`, `spawn_dynamic_child` |
| `bloxide-peers` | `crates/bloxide-peers` | ✅ | `PeerCtrl`, `AddPeer`, `RemovePeer`, `introduce_peers`, `broadcast_to_peers`, `apply_peer_control` |
| `blox-ctx-ping-pong` | `crates/context/blox-ctx-ping-pong` | ✅ | `send_ping`, `send_pong`, `send_initial_ping`, `schedule_resume` action functions |
| `blox-ctx-pool-ref` | `crates/context/blox-ctx-pool-ref` | ✅ | `SpawnRequest`/`SpawnedWorker`, `notify_pool_done`, `broadcast_result` action functions |
| `blox-ctx-rounds` | `crates/context/blox-ctx-rounds` | ✅ | `increment_round` action function |
| `blox-ctx-ticks` | `crates/context/blox-ctx-ticks` | ✅ | `increment_count` action function |
| `bloxide-embassy` | `runtimes/bloxide-embassy` | ✅ | Embassy runtime: `EmbassyRuntime`, `channels!`, `spawn_static_child!`, `spawn_timer!`, task macros |
| `bloxide-tokio` | `runtimes/bloxide-tokio` | — | Tokio runtime: `TokioRuntime`, `channels!`, `spawn_static_child!`, `spawn_timer!`, `SpawnCap`, `KillCapability`, task macros |
| `bloxide-test-runtime` | `runtimes/bloxide-test-runtime` | ✅² | `TestRuntime`: executor-free unit testing; implements `DynamicChannelCap` + `SpawnCap` (kill is a documented no-op) |

¹ Proc-macro crates compile for the host; they have no `no_std` impact on the target binary.
² `no_std` + alloc; the default `std` feature enables thread-local test utilities.

---

## Using bloxide in your project

If you are building actors with bloxide in a separate project, copy the agent guide into your repo so your AI agents understand the framework patterns:

```bash
cp -r skills/building-with-bloxide/ <your-project>/skills/
```

Then reference it from your project's `AGENTS.md`:

```markdown
| Task | Skill |
|---|---|
| Building bloxes with bloxide | `skills/building-with-bloxide/SKILL.md` |
```

The guide covers the four-layer architecture, spec-driven development workflow, step-by-step blox creation, and key invariants.

---

## License

Licensed under the [MIT License](LICENSE).
