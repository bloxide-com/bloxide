# Bloxide

**Hierarchical state machine actors for Rust — runtime-agnostic, from Embassy bare-metal to Tokio.**

[![CI](https://github.com/bloxide-com/bloxide/actions/workflows/lint-and-test.yml/badge.svg)](https://github.com/bloxide-com/bloxide/actions/workflows/lint-and-test.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 2021](https://img.shields.io/badge/rust-2021_edition-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2021/)

Bloxide is a hierarchical state machine (HSM) + actor messaging framework. Domain actors ("bloxes") are generic over `BloxRuntime` so the same state machine logic runs on Embassy *and* Tokio without modification. A separate runtime crate wires channels, spawns tasks, and drives the state machine.

---

## Features

- **Hierarchical state machines** — composite states, event bubbling, entry/exit callbacks, run-to-completion dispatch
- **Declare, don't wire** — state topology, transitions, and actor wiring are declared in `blox.toml` / `system.toml`; `cargo blox` generates the crates, channels, tasks, and `main()`. You write TOML plus plain action functions, never boilerplate
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

You never hand-write actor wiring. You declare the state machine in `blox.toml`, write plain action functions, and wire actors in `system.toml` — `cargo blox` generates everything else.

**1. Declare the state machine** (`bloxes/counter/blox.toml`):

```toml
[[topology.states]]
name = "Ready"
initial = true

# On every Tick: run count_tick, then let guards decide the target
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

**2. Write the action logic** — plain Rust free functions in a context crate; concrete params, no framework traits in the signature:

```rust
// crates/context/blox-ctx-ticks/src/lib.rs
pub fn increment_count(count: &mut u32) -> ActionResult {
    *count += 1;
    ActionResult::Ok
}
```

**3. Wire the system** (`examples/tokio-minimal-demo/system.toml`):

```toml
[system]
runtime = "tokio"
name = "tokio-minimal-demo"

[[actors]]
name = "counter"
blox = "counter-blox"

[[supervision]]
supervisor = "bloxide-supervisor"
strategy = "when_any_done"
children = ["counter"]
```

**4. Generate and run:**

```bash
cargo blox run --example tokio-minimal-demo
```

`cargo blox generate` materializes the channels, `StateMachine` construction, task spawning, supervisor wiring, and `main()` into `target/bloxide-generated/` — generated code you never edit. Each generated crate re-syncs from its TOML source on every build, so plain `cargo` commands work there too. The generated blox crates are generic over `R: BloxRuntime`; the same blox runs on Embassy by setting `runtime = "embassy"` in `system.toml`.

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
│   ├── context/           # composable context crates (blox-ctx-noop, -ping-pong, -pool-ref, -rounds, -ticks)
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

Bloxide uses `cargo blox` for code generation. After declaring actors in `blox.toml` and wiring in `system.toml`:

```bash
cargo install --path crates/tools/cargo-blox
cargo blox generate   # regenerate all boilerplate from blox.toml / system.toml sources
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
