---
name: building-with-bloxide
description: Guide for building bloxes (actors) with the Bloxide framework. Use when creating or modifying actor definitions, message types, context traits, or wiring bloxes into an application. Triggers when working with blox crates, MachineSpec implementations, or the four-layer architecture (messages, context, blox, binary).
metadata:
  short-description: Build bloxes with Bloxide HSM framework
---

# Building with Bloxide

This guide teaches you how to build actors ("bloxes") using the Bloxide framework. It is self-contained and portable — copy it into any project that depends on bloxide crates.

> **NO BACKWARDS COMPATIBILITY.** This project does not preserve backwards
> compatibility — not for APIs, CLIs, config formats, terminology, or
> architecture layers. When something changes, update every call site, every
> doc, and every fixture in the same change, and delete the old form
> completely. Never add legacy aliases, deprecation periods, compatibility
> shims, feature bridges, or "kept for backwards compatibility" notes.

## What Bloxide Is

Bloxide is a `no_std` hierarchical state machine (HSM) + actor messaging framework for Rust. Domain actors ("bloxes") implement the `MachineSpec` trait to define state topologies, event handlers, and context. Blox code is generic over `R: BloxRuntime` so the same state machine runs on Embassy (embedded) and Tokio (std) without modification.

## Four-Layer Architecture

Every bloxide application is built from four layers. Each lives in its own crate with strict dependency rules:

```
Layer 1 — Messages     Pure data enums. No logic, no ActorRef.
Layer 2 — Context       Traits + free action functions taking concrete params.
Layer 3 — Blox           Purely declarative topology, no logic.
Layer 4 — Binary        system.toml + two-stage codegen + optional impl crates.
```

```
Before:  messages → actions → context → impl → blox → (codegen) → binary
After:   messages → context (includes actions) → blox → binary
         impl = optional reusable behavior libraries
```

**Dependency flow:** Messages → Context → Blox. Binary wires everything together via two-stage codegen. Bloxes never import runtime crates or impl crates.

## Layer 1: Messages Crate

Shared message enums used by two or more bloxes. Pure data — no `ActorRef`, no runtime types.

Define messages in `blox.toml` and generate via `cargo blox generate`:

```toml
// crates/messages/ping-pong-messages/blox.toml
[[messages]]
name = "PingPongMsg"
visibility = "pub"

[[messages.variants]]
name = "Ping"

[[messages.variants.fields]]
name = "round"
ty = "u32"

[[messages.variants]]
name = "Pong"

[[messages.variants.fields]]
name = "round"
ty = "u32"

[[messages.variants]]
name = "Resume"
```

After editing `blox.toml`, run `cargo blox generate` to produce `src/generated/messages_pingpongmsg.rs`.

**Rules:**
- Use named struct variants: `Ping { round: u32 }`, not `Ping(u32)`
- Messages used by only one blox can live in that blox's crate
- No `ActorRef` or runtime types in message payloads

## Layer 2: Context Crate

Defines free action functions taking concrete params. Zero concrete implementations in blox crates.

**Action functions** take concrete params, not trait-bounded `&mut C`:

```rust
// crates/context/blox-ctx-rounds/src/lib.rs
pub fn increment_round(round: &mut u32) -> ActionResult {
    *round += 1;
    ActionResult::Ok
}

// crates/context/blox-ctx-ping-pong/src/lib.rs
pub fn send_ping<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    round: u32,
) -> ActionResult {
    ActionResult::from(peer_ref.try_send(self_id, PingPongMsg::Ping(Ping { round })))
}
```

Every action function returns `ActionResult` (uniform contract) so guards can react to failures via `results.any_failed()`. Entry/exit functions are infallible — their result is discarded by the generated wrapper.

**Important:** There is no `B` generic, no `#[delegatable]`, no `#[delegates]`, no accessor traits. State is stored as plain fields on the context struct; action functions take the fields they need as concrete parameters.

## Layer 3: Blox Crate

A blox crate is purely declarative. It contains zero logic — no `actions.rs`, no `Self::` methods, no logging, no computation. It defines:

1. **State topology** via `blox.toml` + `cargo blox generate`
2. **Context struct** via codegen from `[[context.fields]]` and `[[context.uses]]`
3. **Event enum** via `blox.toml` + `cargo blox generate`
4. **`MachineSpec`** with `StateFns` tables generated from `[[topology.transitions]]` entries in `blox.toml`

### Context Struct

Context fields come from `[[context.uses]]` (reference fields) and `[[context.fields]]` (state fields) entries in `blox.toml`. The codegen auto-emits `self_id` (first field). There is no `B` generic, no `behavior: B` field, no `#[delegates(...)]`, no accessor traits. The generated struct looks like:

```rust
// Generated ctx.rs — plain fields, no B generic, no accessor traits
pub struct PingCtx<R: BloxRuntime> {
    pub self_id: ::bloxide_core::ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    pub current_timer: Option<TimerId>,
    pub round: u32,
}
```

**Field sources:**

| Field | Source | Generated |
|-------|--------|-----------|
| `self_id: ActorId` | Auto-emitted (always) | First field, constructor param |
| `foo_ref: ActorRef<M, R>` | `[[context.uses]]` with `field = "foo_ref"` | Constructor parameter |
| `foo_factory: fn(...) -> ...` | `[[context.uses]]` with `role = "ctor"` | Constructor parameter only |
| `round: u32` | `[[context.fields]]` with `name = "round"` | Direct field, zero-initialized in `on_init` |

### Action Declarations

Actions are declared in `[[context.actions]]` entries in `blox.toml`:

```toml
[[context.actions]]
name = "increment_round"
crate = "blox_ctx_rounds"
fields = ["round:mut"]
impl_required = false

[[context.actions]]
name = "process_work"
fields = ["task_id:mut", "result:mut"]
event_payload = "do_work"
impl_required = true
```

Each action specifies:
- `name` — action identifier used in transition/entry/exit declarations
- `fields` — list of ctx fields the action needs, with access mode (`"round:mut"`, `"self_id"`, `"peer_ref:ref"`)
- `event_payload` — optional, name of the extracted payload variable (e.g., `"do_work"`)
- `impl_required` — `true` if the function comes from an impl crate, `false` if from a context crate
- `crate` — optional and informational; the crate providing the function
- `fn_name` — optional, the actual function name if different from `name`

Unknown TOML keys are hard errors (`deny_unknown_fields`). On `[[context.uses]]`, `role` may only be `ctor` (constructor parameter) or `state` (zero-initialized field).

### Event Enum

Define the unified event type in `blox.toml`:

```toml
// crates/bloxes/ping/blox.toml
[event]
name = "PingEvent"

[[event.mailboxes]]
variant = "Msg"
message = "PingPongMsg"
message_path = "ping_pong_messages::PingPongMsg"
```

After running `cargo blox generate`, use the generated event in `src/events.rs`:

```rust
// crates/bloxes/ping/src/events.rs
pub use crate::generated::events::*;
```

This generates:
- `enum PingEvent { Msg(Envelope<PingPongMsg>), Lifecycle(LifecycleCommand) }`
- `EventTag` impl with `MSG_TAG` constant
- `msg_payload()` helper for extracting the inner message
- `From<Envelope<PingPongMsg>>` for stream-to-event conversion
- `LifecycleEvent` impl plus `start()` / `reset()` / `stop()` / `ping()` constructors for lifecycle commands

### State Topology

Define state topology in `blox.toml`:

```toml
// crates/bloxes/ping/blox.toml
[topology]

[[topology.states]]
name = "Operating"
composite = true

[[topology.states]]
name = "Active"
parent = "Operating"
initial = true

[[topology.states]]
name = "Paused"
parent = "Operating"

[[topology.states]]
name = "Error"
error = true
```

- Use `composite = true` for non-leaf states
- Use `parent = "ParentState"` for child states

### Transition Rules

Declare transition rules in `blox.toml` via `[[topology.transitions]]` entries (codegen emits `StateRule` struct literals):

```toml
[[topology.transitions]]
state = "Active"
event = "PingPongMsg::Pong(_)"
target = "stay"
actions = ["Self::increment_round", "Self::forward_ping"]

  [[topology.transitions.guards]]
  condition = "results.any_failed()"
  target = "Error"

  [[topology.transitions.guards]]
  condition = "ctx.round >= MAX_ROUNDS as u32"
  target = "done"

  [[topology.transitions.guards]]
  condition = "ctx.round == PAUSE_AT_ROUND as u32"
  target = "Paused"
```

**Guard context:**
- `ctx` is `&Ctx` (read-only) — direct field access, no trait methods
- `results` is `&ActionResults` — use `results.any_failed()` for send errors
- `stay` keeps current state (no exit/entry); a state name triggers a transition
- `stop` self-suspends to Init (`Decision::Stop`); `done` cleanly ends the task (`Decision::Done`); `reset` goes to `initial_state()`; `fail` goes to the error state (or Init if none is declared)
- Guards are evaluated in order; if none match, the outcome is `Decision::Stay`
- Guard expressions use direct field access (e.g., `ctx.round >= MAX_ROUNDS as u32`)

### Two-Stage Codegen

**Stage 1 — Blox-level (`cargo blox generate`):**

Generates stub action closures (no-op) with real guards. The blox compiles standalone without any impl dependency.

```rust
// generated/spec_skeleton.rs — stub actions, REAL guards
|_ctx, _ev| {
    let _stub = "forward_ping";
    ::bloxide_core::transition::ActionResult::Ok
}
```

**Stage 2 — System-level (`cargo blox build`):**

Reads `system.toml`, resolves impl crates, generates concrete action closures with real function calls. Each closure returns the action function's `ActionResult` verbatim, so a failed send makes `results.any_failed()` true. Guards are unchanged from blox-level (already real).

```rust
// generated/ping_spec_skeleton.rs — concrete, impl inlined
|ctx, _ev| {
    ::blox_ctx_ping_pong::send_ping(ctx.self_id, &ctx.peer_ref, ctx.round)
}
```

### MachineSpec Implementation

The `MachineSpec` impl is generated by the codegen. `Spec<R>` has no `B` type parameter:

```rust
// src/spec.rs
pub use crate::generated::topology::PingState;
use crate::generated::topology::ping_state_handler_table;

impl<R: BloxRuntime> MachineSpec for PingSpec<R> {
    type State = PingState;
    type Event = PingEvent;
    type Ctx = PingCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<PingPongMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = ping_state_handler_table!(Self);

    fn initial_state() -> PingState { PingState::Active }

    fn is_error(state: &PingState) -> bool {
        matches!(state, PingState::Error)
    }

    fn on_init_entry(ctx: &mut PingCtx<R>) {
        ctx.round = 0;
        ctx.current_timer = None;
    }
}
```

## Layer 4: Binary

Creates channels, constructs contexts, spawns tasks. The `system.toml` declares actors, wiring, and impl crates:

```toml
# apps/tokio-demo/system.toml
[[actors]]
name = "ping"
blox = "ping-blox"
# No impl crate needed — all actions come from context crates

  [actors.inject]
  self_ref = { source = "self" }
  peer_ref = { source = "actor", actor = "pong" }
  timer_ref = { source = "actor", actor = "timer" }

[[actors]]
name = "pool"
blox = "pool-blox"
impl_crate = "tokio_pool_demo_impl"  # Provides process_work and spawn functions
```

```rust
// apps/tokio-demo/src/main.rs (generated)
use bloxide_tokio::prelude::*;

// Create channels
let ((ping_ref,), ping_mbox) = bloxide_tokio::channels! { PingPongMsg(16) };
let ping_id = ping_ref.id();

// Build machine — plain fields, no B::default()
let ping_ctx = PingCtx::new(ping_id, pong_ref.clone(), ping_ref.clone(), timer_ref);
let ping_machine = StateMachine::new(ping_ctx);

// Supervised spawning
let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
bloxide_tokio::spawn_static_child!(
    group,
    ping_task(ping_machine, ping_mbox, ping_id),
    ChildPolicy::Reset
);
```

## Code Generation with `cargo blox`

### Installation

```bash
cargo install --path crates/tools/cargo-blox
```

### Creating a New Blox

```bash
cargo blox new my-actor
```

This scaffolds:
- `spec/bloxes/my-actor.md` — the spec document (from the template)
- `crates/bloxes/my-actor/` — the blox crate skeleton (`blox.toml` + generated stubs)

Message and context crates are scaffolded separately when needed
(`cargo blox new-messages`, `cargo blox new-context`).

### Generating Boilerplate

After editing `blox.toml` in any crate, run:

```bash
cargo blox generate
```

Generation runs the linter first and is idempotent — regenerating an unchanged `blox.toml` produces identical output.

**Generate first.** Generated artifacts (`src/generated/`) are gitignored, so a fresh checkout has none — `cargo build`/`cargo test` fail until you run `cargo blox generate`. In a source checkout of the bloxide repo, prefer `cargo run -p cargo-blox -- blox ...` over the installed `cargo blox` binary, which may be stale relative to the checked-out codegen.

This regenerates:
- Message structs and enums from `messages` tables
- Event types from `event` tables
- State topology enums from `topology` tables
- Context struct from `[[context.fields]]` and `[[context.uses]]`
- Stub action closures + real guards from `[[topology.transitions]]` and `[[context.actions]]`

### Building with Code Generation

```bash
cargo blox build    # blox generate + system codegen + cargo build
cargo blox check    # blox generate + system codegen + cargo check (fastest)
cargo blox test     # blox generate + system codegen + cargo test
cargo blox run      # blox generate + system codegen + cargo run
```

## Key Invariants

The canonical invariant list lives in `AGENTS.md` → "Key Invariants". Read it
before writing any Rust code — violating an invariant silently breaks the
architecture. The most relevant ones when creating a blox:

- Blox crates are generic over `R: BloxRuntime`; never import a runtime crate
- Message enums contain plain data only — no `ActorRef`
- Only leaf states may be transition targets
- `on_entry` and `on_exit` are infallible — `fn(&mut Ctx)`, no `Result`
- Actions before guards; guards are pure and use direct field access
- Bubbling is implicit — never add a catch-all rule that returns a parent

## Test Pattern

Use `TestRuntime` for unit tests without an executor. Tests use the blox-level stub `Spec` for guard and transition logic. Action functions are tested by calling the context/impl crate functions directly.

```rust
#[cfg(all(test, feature = "std"))]
mod tests {
    use bloxide_core::{spec::MachineSpec, MachineState, StateMachine};
    use counter_messages::{CounterMsg, Tick};

    // CounterSpec is non-generic — the ctx holds no ActorRef fields.
    // Specs with refs are generic: MySpec<TestRuntime>.
    fn make_machine() -> StateMachine<CounterSpec> {
        let ctx = CounterCtx::new(bloxide_core::next_actor_id!());
        StateMachine::new(ctx)
    }

    #[test]
    fn test_start_enters_ready() {
        let mut machine = make_machine();
        machine.dispatch(CounterEvent::Lifecycle(LifecycleCommand::Start));
        assert!(matches!(machine.current_state(), MachineState::State(CounterState::Ready)));
    }
}
```

For integration tests that need real actions, construct a hand-built `Spec` with real action closures (static closures calling the real functions). No system codegen needed for tests.

```rust
// Test action function directly
let mut task_id = 0u32;
let mut result = 0u32;
let do_work = DoWork { task_id: 2 };
tokio_pool_demo_impl::process_work(&mut task_id, &mut result, &do_work);
assert_eq!(result, 4);  // 2 * 2
```
