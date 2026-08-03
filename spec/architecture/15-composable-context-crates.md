# Composable Context Crates

> **Architecture Update (Phase 1-3, July 2026):** Context crates now contain
> both traits AND action functions. The `B` generic has been eliminated. There
> is no `#[delegatable]` or `#[delegates]`. State fields are plain fields on the
> context struct. Guard expressions use direct field access.

## Problem Statement

Today, every blox defines its context struct from scratch in `blox.toml`. When two bloxes need the same capability — e.g., `self_ref: ActorRef<M, R>` — they independently declare the field, and the action function that uses it needs to be available from a shared crate. The codegen needs to know which fields to import and which action functions map to which fields.

The root issues this design solves:
1. **Action functions belong with the data** — a context crate defines *what data a context has* and *what you do with that data*. The contract and the action belong together.
2. **No duplication** — `send_ping` lives in `blox-ctx-ping-pong`, not duplicated across blox crates.
3. **No manual impls** — the codegen generates the context struct and constructor from `blox.toml` declarations, not hand-written `impl` blocks.
4. **Declarative imports** — the codegen builds imports from explicit `context.imports` declarations plus field-type detection, not string-matching.
5. **Reusability** — a new blox that needs `peer_ref` depends on `blox-ctx-ping-pong` for the action functions.

## Design

### Principle: action functions live in context crates

A context crate owns:
- Free action functions that operate on context data (taking concrete params)
- Type definitions (e.g., `SpawnRequest` / `SpawnedWorker` in `blox-ctx-pool-ref`) when needed
- No `B` generic, no `#[delegatable]`, no accessor traits, no forwarding impls

### Four-layer crate model

```
bloxide-core          ← engine (required by all bloxes)
  ActorId, ActorRef, BloxRuntime, MachineSpec, StateFns, StateRule

service crates        ← infrastructure capabilities (optional)
  bloxide-timer        ← set_timer, cancel_timer, cancel_timer_by_id (action functions)

domain context crates ← domain-specific data composition (optional)
  blox-ctx-ping-pong   ← send_ping, send_pong, send_initial_ping, schedule_resume
  blox-ctx-pool-ref    ← notify_pool_done, broadcast_result action functions;
                         SpawnRequest/SpawnedWorker spawn protocol types
  blox-ctx-rounds      ← increment_round action function
  blox-ctx-ticks       ← increment_count action function

blox crates           ← TOML → codegen (depend on context crates)
  ping-blox, pong-blox, pool-blox, worker-blox, counter-blox
```

### What stays in bloxide-core

Only what *every* blox needs, no exceptions:
- `ActorId`, `ActorRef`, `BloxRuntime`
- `MachineSpec`, `StateFns`, `StateTopology`
- `StateRule`, `TransitionRule` (transition rules are declared in `blox.toml` via `[[topology.transitions]]` and emitted by `bloxide-codegen`)
- `ActionResult`, `StateRule`

### Service-level crates

Service crates follow the `bloxide-timer` model: action functions live in the crate. A blox pulls in the crate if it needs that service.

#### `blox-ctx-ping-pong`

Provides messaging primitives — action functions that send messages via `ActorRef`s. Both `self_ref` and `peer_ref` are `ActorRef<M, R>` where `M` varies per blox. One crate, action functions for both:

```rust
// crates/blox-ctx-ping-pong/src/lib.rs
use bloxide_core::{BloxRuntime, messaging::ActorRef, transition::ActionResult, ActorId};
use ping_pong_messages::{Ping, PingPongMsg};

/// Action function: send a Ping message to the peer.
pub fn send_ping<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    round: u32,
) -> ActionResult {
    ActionResult::from(peer_ref.try_send(self_id, PingPongMsg::Ping(Ping { round })))
}
```

The codegen emits these as plain fields on the context struct:
- `self_ref: ActorRef<M, R>` → constructor parameter
- `peer_ref: ActorRef<M, R>` → constructor parameter

### Domain context crates

Domain context crates own action functions for domain-specific capabilities.

#### Single-field action functions

For simple capabilities (one field, one action function), the context crate provides only the action function. The blox declares the field in `blox.toml` and the codegen emits it as a plain field:

```rust
// crates/blox-ctx-pool-ref/src/lib.rs
use bloxide_core::{BloxRuntime, messaging::ActorRef, transition::ActionResult, ActorId};
use pool_messages::{PoolMsg, WorkDone};

/// Action function: notify the pool that work is done.
pub fn notify_pool_done<R: BloxRuntime>(
    self_id: ActorId,
    pool_ref: &ActorRef<PoolMsg, R>,
    task_id: u32,
    result: u32,
) -> ActionResult {
    ActionResult::from(pool_ref.try_send(
        self_id,
        PoolMsg::WorkDone(WorkDone { worker_id: self_id, task_id, result }),
    ))
}
```

#### State action functions

State action functions like `increment_round` are plain free functions — no `#[delegatable]` macro, no `B` generic, no trait. They take the field they operate on as a concrete parameter and return `ActionResult`:

```rust
// crates/blox-ctx-rounds/src/lib.rs
pub fn increment_round(round: &mut u32) -> ActionResult {
    *round += 1;
    ActionResult::Ok
}
```

The codegen generates a plain field on the context struct and a wrapper closure that passes the field to the action function, returning its `ActionResult` verbatim:

```rust
// Generated by codegen — plain field, no trait impl
pub struct PingCtx<R: BloxRuntime> {
    pub round: u32,
    // ... other fields ...
}

// Generated wrapper closure (in spec_skeleton.rs)
|ctx, _ev| blox_ctx_rounds::increment_round(&mut ctx.round)
```

### blox.toml schema

The context section uses `[[context.uses]]` for composable field declarations and `[[context.fields]]` for state fields:

```toml
[context]
name = "PingCtx"
generics = "<R: BloxRuntime>"
on_init = "ctx.round = 0; ctx.current_timer = None;"
imports = [
    "ping_pong_messages::PingPongMsg",
    "bloxide_timer::{TimerCommand, TimerId}",
]

# Reference fields — codegen emits plain fields + constructor params.
# The `crate` key is optional and informational (scaffolding/visualization);
# imports come from `imports` above, not from `uses` entries.
[[context.uses]]
crate = "blox_ctx_ping_pong"
field = "peer_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

[[context.uses]]
crate = "blox_ctx_ping_pong"
field = "self_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

# Multi-field form — several fields contributed by one domain crate
# (from pool-blox, feature-gated under `dynamic`)
[[context.uses]]
crate = "pool_messages"
feature = "dynamic"
fields = [
    { name = "spawn_fn", ty = "SpawnFn<R, SpawnRequest<PeerCtrl<WorkerMsg, R>, R>>", role = "ctor" },
    { name = "spawn_queue", ty = "Vec<u32>", role = "state" },
]

# State fields — plain fields on the context struct
[[context.fields]]
name = "current_timer"
type = "Option<TimerId>"

[[context.fields]]
name = "round"
type = "u32"

# Action declarations — what actions the blox calls, not what they do.
# `kind` is required and validated against the use site ("entry"/"exit"/"transition").
[[context.actions]]
name = "increment_round"
crate = "blox_ctx_rounds"
kind = "transition"
fields = ["round:mut"]
impl_required = false

[[context.actions]]
name = "send_initial_ping"
crate = "blox_ctx_ping_pong"
kind = "entry"
fields = ["self_id", "peer_ref:ref", "round:mut"]
impl_required = false
```

### Field roles

Each context field has an explicit role that tells the codegen what to emit
(`role` is validated — `ctor` and `state` are the only accepted values):

| Role | Codegen behavior |
|------|-----------------|
| `ctor` | Add field, add field to constructor signature (constructor parameter) |
| `state` | Add field, zero-initialize via `Default::default()` in `Ctx::new()` |

`self_id` is auto-emitted by the codegen — it is not declared in `blox.toml`.

### What the codegen does with `context.uses`

For each `uses` entry, the codegen:

1. **Adds fields** to the generated struct definition
2. **Adds fields to constructor** — fields with `role = "ctor"` become constructor parameters

It does **not** emit imports from `uses` entries — the `crate` key is
informational (used by scaffolding and visualization). Generated `use`
statements come from `context.imports` / `context.feature_imports` (explicit
1:1 mappings from the TOML) plus auto-detected framework imports
(`BloxRuntime`, `ActorRef`) derived from the field types and generics. Beyond
that auto-detection, the codegen **never guesses imports**.

### Guard expression translation

Guards in `blox.toml` are pure expressions over ctx fields and `ActionResults`. The blox-level codegen translates them to direct field access:

| `blox.toml` expression | Generated Rust |
|---|---|
| `ctx.round >= MAX_ROUNDS as u32` | `ctx.round >= MAX_ROUNDS as u32` (direct field access) |
| `ctx.pending == 0` | `ctx.pending == 0` |
| `ctx.spawn_in_flight \|\| !ctx.spawn_queue.is_empty()` | `ctx.spawn_in_flight \|\| !ctx.spawn_queue.is_empty()` |
| `results.any_failed()` | `results.any_failed()` (unchanged — method on `ActionResults`) |

The codegen parser:
1. Leaves `ctx.field` direct field access unchanged (already correct)
2. Leaves `results.*()` calls unchanged (they're methods on `ActionResults`)
3. Leaves boolean operators (`&&`, `||`, `!`) unchanged

### Visual Editor Integration

The blox.toml `[[context.uses]]` and `[[context.fields]]` entries drive a visual editor where you:
- Add context fields by picking from a library of context crates (dropdown)
- Each context crate shows what fields + action functions it provides
- Set field roles (ctor / state) via dropdown
- The codegen assembles the struct, imports, and constructor

The only hand-written Rust is action function bodies (in context/impl crates) and guard predicate bodies (in `blox.toml` expressions).
