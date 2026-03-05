# Bloxide

<p align="center">
  <img src="bloxide.jpg" alt="Bloxide" width="600" />
</p>

**Hierarchical state machine actors for Rust — from bare-metal Embassy to Tokio, with the same domain code.**

[![CI](https://github.com/bloxide-com/bloxide/actions/workflows/lint-and-test.yml/badge.svg)](https://github.com/bloxide-com/bloxide/actions/workflows/lint-and-test.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 2021](https://img.shields.io/badge/rust-2021_edition-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2021/)
[![no_std](https://img.shields.io/badge/no__std-core-brightgreen.svg)](#crates)

Bloxide is a `no_std` [hierarchical state machine (HSM)][hsm] + actor messaging framework. Domain actors ("bloxes") are fully runtime-agnostic — generic over `R: BloxRuntime` — so the same state machine code runs on Embassy bare-metal *and* Tokio without modification. A separate runtime crate wires channels, spawns tasks, and drives the engine.

[hsm]: https://en.wikipedia.org/wiki/UML_state_machine

---

## Why bloxide?

| Problem | Bloxide answer |
|---|---|
| State machine logic tangled with async plumbing | `MachineSpec` trait keeps HSM logic separate from runtime |
| Hard to test actors without an executor | `TestRuntime` in `bloxide-core` runs actors synchronously in unit tests |
| Actors break when you swap Embassy for Tokio | Blox crates are generic over `R: BloxRuntime`; swap the runtime crate, keep the blox |
| Event-handling scattered across match arms | `transitions!` macro composes actions + guards declaratively, state by state |
| Manual supervision boilerplate | `bloxide-supervisor` provides a reusable OTP-style supervisor out of the box |
| `defmt` vs `log` vs nothing | `bloxide-log` feature flags select the backend at compile time; bloxes compile with zero logging if you want |

---

## Features

- **Full HSM engine** — composite states, leaf states, event bubbling, lowest-common-ancestor (LCA) exit/entry ordering, run-to-completion dispatch
- **`no_std` core** — `bloxide-core` and all blox crates compile for bare-metal (`#![no_std]`); `alloc`/`std` features available
- **Runtime-agnostic actors** — blox crates are generic over `R: BloxRuntime`; no executor import anywhere in domain code
- **Dual runtime support** — Embassy (embedded, `static` channels) and Tokio (server/desktop, dynamic channels) ship as first-class runtimes
- **OTP-style supervision** — `bloxide-supervisor` provides restart policies, group-shutdown strategies, and lifecycle reporting with zero boilerplate in child actors
- **Proc macros** — `#[derive(StateTopology)]`, `#[derive(BloxCtx)]`, `transitions!`, `root_transitions!` reduce state machine declaration to signal-to-noise ratio near zero
- **Feature-gated logging** — `bloxide-log` provides `blox_log_info!` / `blox_log_debug!` / etc.; enable `log` or `defmt` in the binary crate; blox crates stay dependency-free
- **Timer service** — `bloxide-timer` models timers as messages; `set_timer` / `cancel_timer` actions work identically on Embassy and Tokio
- **Testable without an executor** — `TestRuntime` in `bloxide-core` dispatches events synchronously; write acceptance tests for every state transition before touching async code

---

## Quick look

A blox is an enum of states plus a `MachineSpec` implementation. Here is the Ping actor from the included example — it exchanges messages with Pong, pauses mid-run via a timer, and signals done after `MAX_ROUNDS`:

```rust
// State topology — proc macro generates parent/child navigation
#[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
#[handler_fns(OPERATING_FNS, ACTIVE_FNS, PAUSED_FNS, DONE_FNS, ERROR_FNS)]
pub enum PingState {
    #[composite]             Operating,   // composite parent
    #[parent(Operating)]     Active,      // leaf — sends pings
    #[parent(Operating)]     Paused,      // leaf — waits for timer
    Done,                                 // terminal
    Error,                                // error
}

// Active state — enters, sends a ping, handles pong with actions + guard
const ACTIVE_FNS: StateFns<PingSpec<R, B>> = StateFns {
    on_entry: &[increment_round, log_round, send_initial_ping],
    on_exit:  &[],
    transitions: transitions![
        PingPongMsg::Pong(pong) => {
            actions [log_pong_received, forward_ping]
            guard(ctx, results) {
                results.any_failed()                  => PingState::Error,
                ctx.round() >= MAX_ROUNDS             => PingState::Done,
                ctx.round() == PAUSE_AT_ROUND         => PingState::Paused,
                _                                     => PingState::Active,
            }
        },
    ],
};

// Paused state — sets a timer on entry, transitions back to Active on Resume
const PAUSED_FNS: StateFns<PingSpec<R, B>> = StateFns {
    on_entry: &[schedule_pause_timer],
    on_exit:  &[cancel_pause_timer],
    transitions: transitions![
        PingPongMsg::Resume(_) => {
            actions [forward_ping]
            transition PingState::Active
        },
    ],
};

// MachineSpec wires everything together
impl<R: BloxRuntime, B: /* behavior traits */> MachineSpec for PingSpec<R, B> {
    type State     = PingState;
    type Event     = PingEvent;
    type Ctx       = PingCtx<R, B>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<PingPongMsg>,);

    fn initial_state() -> PingState { PingState::Active }
    fn is_terminal(s: &PingState)   -> bool { matches!(s, PingState::Done) }
    fn is_error(s: &PingState)      -> bool { matches!(s, PingState::Error) }
    fn on_init_entry(ctx: &mut PingCtx<R, B>) { ctx.behavior = B::default(); }
}
```

The identical `PingSpec` runs on Embassy:

```rust
bloxide_embassy::actor_task_supervised!(ping_task, PingSpec<EmbassyRuntime, PingBehavior>);
```

…and on Tokio:

```rust
bloxide_tokio::actor_task_supervised!(ping_task, PingSpec<TokioRuntime, PingBehavior>);
```

No changes to the blox crate.

---

## Architecture

Bloxide separates concerns across five layers. Domain code lives in the top three; the bottom two are swapped per target:

```
┌──────────────────────────────────────────────────────────────┐
│  Binary / Wiring  (channels!, spawn_child!, ActorRef wiring)  │
├──────────────────────────────────────────────────────────────┤
│  Impl Crate       (concrete behavior types, injected at wire) │
├──────────────────────────────────────────────────────────────┤
│  Blox Crate       (MachineSpec, Ctx, StateFns — no_std)       │
├──────────────────────────────────────────────────────────────┤
│  Actions Crate    (accessor traits + generic action fns)      │
├──────────────────────────────────────────────────────────────┤
│  Messages Crate   (plain data enums/structs — no deps)        │
└──────────────────────────────────────────────────────────────┘
         ↑ all layers depend on ↓
┌────────────────────┐   ┌──────────────────┐
│  bloxide-core      │   │  bloxide-timer   │
│  (HSM engine,      │   │  bloxide-super-  │
│   no_std)          │   │  visor  (no_std) │
└────────────────────┘   └──────────────────┘
         ↑ implemented by ↓
┌────────────────────┐   ┌──────────────────┐
│ bloxide-embassy    │   │  bloxide-tokio   │
│ (embedded target)  │   │  (std target)    │
└────────────────────┘   └──────────────────┘
```

The `BloxRuntime` trait is the only abstraction blox crates depend on. It exposes `Sender`, `Receiver`, and `Stream` associated types. Runtime crates implement it. Blox crates never import a runtime.

### HSM dispatch

Events are dispatched run-to-completion from the active leaf state up through its ancestors. If no rule matches at a given level, the event bubbles implicitly to the parent — no explicit catch-all needed. On transition, the engine computes the LCA, fires `on_exit` handlers from the source leaf to (but not including) the LCA, then fires `on_entry` handlers from below the LCA down to the target leaf.

```
Operating
├── Active   ←── current state
└── Paused

Transition Active → Done:
  Active::on_exit  →  Operating::on_exit  →  Done::on_entry
```

### Supervision

`bloxide-supervisor` provides an OTP-inspired supervisor with no custom code required:

```rust
let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
bloxide_embassy::spawn_child!(spawner, group,
    ping_task(ping_machine, ping_mbox, ping_id),
    ChildPolicy::Restart { max: 1 }      // restart on failure, up to once
);
bloxide_embassy::spawn_child!(spawner, group,
    pong_task(pong_machine, pong_mbox, pong_id),
    ChildPolicy::Stop                    // stop permanently when done
);
let (children, sup_notify_rx) = group.finish();
let sup_ctx = SupervisorCtx::new(sup_id, children);
```

Child actors never reference the supervisor. Lifecycle signals flow through a dedicated channel managed entirely by the runtime.

---

## Crates

| Crate | Path | `no_std` | Purpose |
|---|---|:---:|---|
| `bloxide-core` | `crates/bloxide-core` | ✅ | HSM engine, `MachineSpec`, `BloxRuntime`, `StateMachine`, `ActorRef`, `TestRuntime` |
| `bloxide-macros` | `crates/bloxide-macros` | ✅¹ | `#[derive(StateTopology)]`, `#[derive(BloxCtx)]`, `transitions!`, `#[blox_event]` |
| `bloxide-log` | `crates/bloxide-log` | ✅ | Feature-gated logging macros (`log` / `defmt` / no-op); blox crates depend with no features |
| `bloxide-timer` | `crates/bloxide-timer` | ✅ | `TimerCommand`, `TimerQueue`, `set_timer`, `cancel_timer`, `TimerService` trait |
| `bloxide-supervisor` | `crates/bloxide-supervisor` | ✅ | `SupervisorSpec`, `ChildGroup`, `ChildPolicy`, `GroupShutdown`, `ChildLifecycleEvent` |
| `bloxide-spawn` | `crates/bloxide-spawn` | — | Dynamic actor spawning and peer introduction |
| `bloxide-embassy` | `runtimes/bloxide-embassy` | — | Embassy runtime: `EmbassyRuntime`, `channels!`, `actor_task!`, `spawn_child!`, `timer_task!` |
| `bloxide-tokio` | `runtimes/bloxide-tokio` | — | Tokio runtime: `TokioRuntime`, `channels!`, `actor_task!`, `spawn_child!`, `spawn_timer!` |

¹ Proc-macro crates compile for the host; they have no `no_std` impact on the target binary.

### Feature flags (`bloxide-core`)

| Flag | Effect |
|---|---|
| _(default)_ | `heapless` fixed-capacity containers; pure `no_std` |
| `alloc` | Use `alloc::vec::Vec` for state paths |
| `std` | Use `std::vec::Vec`; implies `alloc` |
| `tracing` | Emit `tracing::trace!` at every entry, exit, and transition |

---

## Running the examples

### Tokio (desktop / CI)

```bash
# Ping-pong with OTP supervision, timer-driven pause, and full HSM tracing
RUST_LOG=trace cargo run --bin tokio-demo

# Worker pool with dynamic spawning
RUST_LOG=info cargo run --bin tokio-pool-demo
```

### Embassy (std target, simulates embedded)

```bash
RUST_LOG=trace cargo run --bin embassy-demo
```

Both demos exercise:

1. Deep state hierarchy — `Operating(Active, Paused)` → `Done`
2. Event bubbling — `Paused` absorbs stray `Pong` via its composite parent
3. LCA exit ordering — `Active::on_exit` *and* `Operating::on_exit` fire on `→ Done`
4. Timer-driven transitions — `Paused::on_entry` schedules a resume timer
5. OTP supervision — supervisor restarts Ping once on error, stops Pong on completion
6. Clean shutdown — supervisor self-terminates via `Guard::Reset` after all children are done

---

## Testing bloxes without an executor

`bloxide-core` ships a `TestRuntime` that satisfies `DynamicChannelCap`. Tests dispatch events synchronously and inspect state directly:

```rust
use bloxide_core::test_utils::*;

let ctx = PingCtx::new(/* ... */);
let mut machine = StateMachine::<PingSpec<TestRuntime, MockBehavior>>::new(ctx);
machine.start();

let outcome = machine.dispatch(ping_event(PingPongMsg::Pong(Pong { round: 1 })));
assert!(matches!(outcome, DispatchOutcome::Transition(PingState::Active)));
```

No executor, no async, no spawning. Each blox has acceptance tests derived directly from its spec before any runtime integration.

---

## Design principles

1. **`bloxide-core` is `no_std`** — zero OS, Tokio, or Embassy imports. Only `futures-core` is permitted as a runtime library dep.
2. **Blox crates are runtime-agnostic** — generic over `R: BloxRuntime`. Never import `bloxide-embassy` or `bloxide-tokio` from a blox crate.
3. **No runtime types in messages** — domain enums contain plain data only; `ActorRef` lives in `Ctx`, never in a message variant.
4. **Actions before guards** — `transitions!` enforces `actions: fn(&mut Ctx, &Event)` then `guard: fn(&Ctx, &ActionResults, &Event) -> Guard`. Side effects are separated from decisions; the borrow checker enforces the ordering.
5. **Bubbling is implicit** — states with no matching rule automatically bubble to the parent. An empty `transitions: &[]` means "handle nothing; let everything bubble." No explicit catch-all needed.
6. **`on_entry` / `on_exit` are infallible** — they are `fn(&mut Ctx)`. Fallible work belongs in a `TransitionRule`'s `actions` function, where it returns `ActionResult::Err` and the guard can route to an error state.
7. **Spec-driven development** — write the blox spec first (`spec/bloxes/<name>.md`), write `TestRuntime` tests next, implement `MachineSpec` last.

---

## Repository layout

```
bloxide/
├── crates/            # core library crates
│   ├── bloxide-core/
│   ├── bloxide-log/
│   ├── bloxide-macros/
│   ├── bloxide-spawn/
│   ├── bloxide-supervisor/
│   └── bloxide-timer/
├── runtimes/          # runtime implementations
│   ├── bloxide-embassy/
│   └── bloxide-tokio/
├── examples/          # worked examples (ping-pong, pool/worker)
│   ├── messages/      # shared message crates
│   ├── actions/       # action trait crates
│   ├── bloxes/        # ping, pong, worker, pool
│   ├── embassy-demo-impl/ # concrete behavior types (e.g. PingBehavior)
│   ├── embassy-demo/
│   ├── tokio-demo/
│   └── tokio-pool-demo/
├── spec/              # architecture docs and per-blox specs
│   ├── architecture/  # 00–11 design docs
│   ├── bloxes/        # spec/bloxes/ping.md, pong.md, supervisor.md
│   └── templates/     # blox-spec template
```

---

## License

Licensed under the [MIT License](LICENSE).
