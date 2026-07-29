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
4. **Declarative imports** — the codegen knows imports from `[[context.uses]]` entries, not string-matching.
5. **Reusability** — a new blox that needs `peer_ref` depends on `blox-ctx-ping-pong` for the action functions.

## Design

### Principle: action functions live in context crates

A context crate owns:
- Free action functions that operate on context data (taking concrete params)
- Type definitions (e.g., `WorkerSpawnFn<R>`) when needed
- No `B` generic, no `#[delegatable]`, no accessor traits, no forwarding impls

### Four-layer crate model

```
bloxide-core          ← engine (required by all bloxes)
  ActorId, ActorRef, BloxRuntime, MachineSpec, StateFns, StateRule

service crates        ← infrastructure capabilities (optional)
  blox-ctx-ping-pong   ← send_ping, send_pong, send_initial_ping (action functions)
  bloxide-timer       ← set_timer, cancel_timer (action functions)

domain context crates ← domain-specific data composition (optional)
  blox-ctx-pool-ref   ← notify_pool_done action function
  blox-ctx-rounds     ← increment_round action function
  blox-ctx-ping-pong ← schedule_resume, cancel_timer_by_id action functions
  blox-ctx-ticks        ← increment_count action function

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
use bloxide_core::{BloxRuntime, messaging::ActorRef, ActorId};
use ping_pong_messages::PingPongMsg;

/// Action function: send a Ping message to the peer.
pub fn send_ping<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    round: u32,
) {
    let _ = peer_ref.try_send(self_id, PingPongMsg::Ping(Ping { round }));
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
use bloxide_core::{BloxRuntime, messaging::ActorRef, ActorId};
use pool_messages::PoolMsg;

/// Action function: notify the pool that work is done.
pub fn notify_pool_done<R: BloxRuntime>(
    self_id: ActorId,
    pool_ref: &ActorRef<PoolMsg, R>,
    task_id: u32,
    result: u32,
) {
    let _ = pool_ref.try_send(self_id, PoolMsg::WorkDone(WorkDone { task_id, result }));
}
```

#### State action functions

State action functions like `increment_round` are plain free functions — no `#[delegatable]` macro, no `B` generic, no trait. They take the field they operate on as a concrete parameter:

```rust
// crates/blox-ctx-rounds/src/lib.rs
pub fn increment_round(round: &mut u32) {
    *round += 1;
}
```

The codegen generates a plain field on the context struct and a wrapper closure that passes the field to the action function:

```rust
// Generated by codegen — plain field, no trait impl
pub struct PingCtx<R: BloxRuntime> {
    pub round: u32,
    // ... other fields ...
}

// Generated wrapper closure (in spec_skeleton.rs)
|ctx| { blox_ctx_rounds::increment_round(&mut ctx.round); }
```

### blox.toml schema

The context section uses `[[context.uses]]` for field declarations (with action function imports) and `[[context.fields]]` for state fields:

```toml
[context]
name = "PingCtx"
generics = "<R: BloxRuntime>"
on_init = "ctx.round = 0; ctx.current_timer = None;"

# Reference fields — codegen emits plain fields + constructor params
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

# State fields — plain fields on the context struct
[[context.fields]]
name = "current_timer"
type = "Option<TimerId>"

[[context.fields]]
name = "round"
type = "u32"

# Action declarations — what actions the blox calls, not what they do
[[context.actions]]
name = "increment_round"
crate = "blox_ctx_rounds"
kind = "transition"
fields = ["round:mut"]
impl_required = false

[[context.actions]]
name = "send_initial_ping"
crate = "blox_ctx_ping_pong"
kind = "transition"
fields = ["self_id", "peer_ref:ref", "round"]
impl_required = false
```

### Field roles

Each context field has an explicit role that tells the codegen what to emit:

| Role | Codegen behavior |
|------|-----------------|
| `ctor` | Add field, emit import, add field to constructor signature (constructor parameter) |
| `state` (from `[[context.fields]]`) | Add field, zero-initialize in `on_init` |

`self_id` is auto-emitted by the codegen — it is not declared in `blox.toml`.

### What the codegen does with `context.uses`

For each `uses` entry, the codegen:

1. **Adds fields** to the generated struct definition
2. **Emits imports** — `use {crate}::*;` for the action functions
3. **Adds fields to constructor** — fields with `role = "ctor"` become constructor parameters

The codegen **never guesses imports**. Every import is a direct 1:1 mapping from the TOML.

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
- Set field roles (accessor / ctor / state) via dropdown
- The codegen assembles the struct, imports, and constructor

The only hand-written Rust is action function bodies (in context/impl crates) and guard predicate bodies (in `blox.toml` expressions).
