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
- **Dynamic actors** — spawn new actors at runtime with factory injection and automatic peer introduction (`bloxide-spawn`)

---

## Start Here

- Read [START_HERE.md](START_HERE.md) for the three-layer principle, five-layer application structure, and two-tier trait system in one place.
- Use [skills/building-with-bloxide/SKILL.md](skills/building-with-bloxide/SKILL.md) as the step-by-step build workflow.
- Keep [skills/building-with-bloxide/reference.md](skills/building-with-bloxide/reference.md) open as the macro and API reference while you build.
- For the smallest runnable example, start with `cargo run --example tokio-minimal-demo` (now fully five-layered via `counter-*` crates).

---

## Quick look

A blox implements `MachineSpec` to define states, transitions, and context. At startup the runtime creates channels, builds `StateMachine` instances, and spawns tasks. Here is a trimmed view of the Tokio demo wiring two supervised ping-pong actors:

```rust
// Create typed channels for each actor
let ((ping_ref,), ping_mbox) = bloxide_tokio::channels! { PingPongMsg(16) };
let ((pong_ref,), pong_mbox) = bloxide_tokio::channels! { PingPongMsg(16) };

// Build state machines — PingSpec and PongSpec are runtime-agnostic MachineSpec impls
let ping_machine = StateMachine::new(PingCtx::new(/* peer refs, timer, behavior */));
let pong_machine = StateMachine::new(PongCtx::new(/* peer ref */));

// Supervise both actors
let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
bloxide_tokio::spawn_child!(group, ping_task(ping_machine, ping_mbox, ping_id),
    ChildPolicy::Restart { max: 1 });
bloxide_tokio::spawn_child!(group, pong_task(pong_machine, pong_mbox, pong_id),
    ChildPolicy::Stop);

// Start the supervisor and run until shutdown
let _sup_control_ref = group.control_ref();
let (children, sup_notify_rx, sup_control_rx) = group.finish();
let sup_ctx = SupervisorCtx::new(bloxide_tokio::next_actor_id!(), children);
let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
sup_machine.start();
run_root(sup_machine, (sup_notify_rx, sup_control_rx)).await;
```

The blox crates (`PingSpec`, `PongSpec`) are generic over `R: BloxRuntime` — the same code runs on Embassy by swapping `TokioRuntime` for `EmbassyRuntime`.

**Unsupervised vs supervised:** `run_actor_to_completion` calls `machine.start()` internally and runs until `DispatchOutcome::Transition`/`Started` hits a terminal or error state, or `DispatchOutcome::Reset`. Supervised actors use `run_supervised_actor`, which waits for `LifecycleCommand::Start` from the parent before calling `start()`; the supervisor drives `reset()` via Reset/Stop. See `tokio-minimal-demo` for unsupervised layered wiring, and `tokio-demo` for supervised layered wiring.

---

## Repository layout

```
bloxide/
├── crates/            # framework + layered application crates
│   ├── bloxide-core/      # HSM engine, MachineSpec, BloxRuntime, std-gated TestRuntime
│   ├── bloxide-log/       # feature-gated logging macros (log / defmt / no-op)
│   ├── bloxide-macros/    # proc macros: #[derive(StateTopology)], transitions!, #[blox_event]
│   ├── bloxide-spawn/     # dynamic actor spawning and peer introduction
│   ├── bloxide-supervisor/ # reusable OTP-style supervisor
│   ├── bloxide-timer/     # timer service: set_timer / cancel_timer
│   ├── messages/          # shared message crates (ping-pong, pool, counter)
│   ├── actions/           # action trait crates (ping-pong, pool, counter)
│   ├── bloxes/            # ping, pong, worker, pool, counter
│   └── impl/              # concrete behavior/factory crates for wiring demos
├── runtimes/          # runtime implementations
│   ├── bloxide-embassy/   # Embassy runtime (embedded target)
│   └── bloxide-tokio/     # Tokio runtime (std target)
├── examples/          # top-level runnable example entrypoints
│   ├── embassy-demo.rs
│   ├── tokio-minimal-demo.rs
│   ├── tokio-demo.rs
│   └── tokio-pool-demo.rs
├── spec/              # architecture docs and per-blox specs
│   ├── architecture/      # numbered design docs
│   ├── bloxes/            # per-blox specs (ping, pong)
│   └── templates/         # blox-spec template
└── .github/workflows/ # CI: copyright, fmt, clippy, tests, rustdoc
```

---

## Running the examples

```bash
# Minimal single-actor Tokio example
cargo run --example tokio-minimal-demo

# Ping-pong with OTP supervision, timer-driven pause, and full HSM tracing
RUST_LOG=trace cargo run --example tokio-demo

# Worker pool with dynamic spawning
RUST_LOG=info cargo run --example tokio-pool-demo

# Embassy (std target, simulates embedded)
RUST_LOG=trace cargo run --example embassy-demo
```

---

## Crates

| Crate | Path | `no_std` | Purpose |
|---|---|:---:|---|
| `bloxide-core` | `crates/bloxide-core` | ✅ | HSM engine, `MachineSpec`, `BloxRuntime`, `StateMachine`, std-gated `TestRuntime` |
| `bloxide-macros` | `crates/bloxide-macros` | ✅¹ | `#[derive(StateTopology)]`, `#[derive(BloxCtx)]`, `#[derive(EventTag)]`, `transitions!`, `#[blox_event]` |
| `bloxide-log` | `crates/bloxide-log` | ✅ | Feature-gated logging macros (`log` / `defmt` / no-op) |
| `bloxide-timer` | `crates/bloxide-timer` | ✅ | `TimerCommand`, `TimerQueue`, `set_timer`, `cancel_timer` |
| `bloxide-supervisor` | `crates/bloxide-supervisor` | ✅ | `SupervisorSpec`, `ChildGroup`, `ChildPolicy`, `GroupShutdown` |
| `bloxide-spawn` | `crates/bloxide-spawn` | ✅ | Dynamic actor spawning and peer introduction |
| `bloxide-embassy` | `runtimes/bloxide-embassy` | ✅ | Embassy runtime: `EmbassyRuntime`, `channels!`, `next_actor_id!`, `actor_task!`, `actor_task_supervised!`, `root_task!`, `timer_task!`, `spawn_child!`, `spawn_timer!`, `run_root` |
| `bloxide-tokio` | `runtimes/bloxide-tokio` | — | Tokio runtime: `TokioRuntime`, `channels!`, `next_actor_id!`, `actor_task!`, `actor_task_supervised!`, `root_task!`, `spawn_child!`, `spawn_timer!`, `run_root` |

¹ Proc-macro crates compile for the host; they have no `no_std` impact on the target binary.

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

The guide covers the five-layer architecture, spec-driven development workflow, step-by-step blox creation, and key invariants.

---

## License

Licensed under the [MIT License](LICENSE).
