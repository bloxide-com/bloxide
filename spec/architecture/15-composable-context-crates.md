# Composable Context Crates

> **Architecture Update (Phase 1-3, July 2026):** Context crates now contain
> both traits AND action functions. The `B` generic has been eliminated. There
> is no `#[delegatable]` or `#[delegates]`. State fields are plain fields on the
> context struct. Guard expressions use direct field access.

## Problem Statement

Today, every blox defines its context struct from scratch in `blox.toml`. When two bloxes need the same capability — e.g., `self_ref: ActorRef<M, R>` — they independently define the field, and the trait (`HasSelfRef`) needs to be available from a shared crate. The codegen needs to know which traits to import and which fields map to which trait methods.

The root issues this design solves:
1. **Trait definitions belong with the data** — a context trait is a contract about *what data a context has*. An action function is *what you do with that data*. The contract belongs with the data.
2. **No trait duplication** — `HasSelfRef` lives in `bloxide-messaging`, not duplicated across action crates.
3. **No manual impls** — the codegen generates trait impls from `blox.toml` declarations, not hand-written `impl` blocks.
4. **Declarative imports** — the codegen knows imports from `[[context.uses]]` entries, not string-matching.
5. **Reusability** — a new blox that needs `peer_ref` depends on `bloxide-messaging` for the trait + action functions.

## Design

### Principle: trait definitions and action functions live together in context crates

A context crate owns:
- The trait definition (contract about what data a context has)
- Free action functions that operate on that data (taking concrete params)
- No `B` generic, no `#[delegatable]`, no forwarding impls

### Four-layer crate model

```
bloxide-core          ← engine (required by all bloxes)
  HasSelfId, ActorId, ActorRef, BloxRuntime, MachineSpec, StateFns, StateRule

service crates        ← infrastructure capabilities (optional)
  bloxide-messaging   ← HasSelfRef<R, M>, HasPeerRef<R, M> + send_ping, send_pong, send_initial_ping
  bloxide-timer       ← HasTimerRef<R>, set_timer, cancel_timer

domain context crates ← domain-specific data composition (optional)
  blox-ctx-workers    ← HasWorkers<R>, HasWorkerFactory<R>
  blox-ctx-pool-ref   ← HasPoolRef<R> + notify_pool_done
  blox-ctx-rounds     ← CountsRounds + increment_round
  blox-ctx-current-timer ← HasCurrentTimer + schedule_resume, cancel_timer_by_id
  blox-ctx-current-task  ← HasCurrentTask
  blox-ctx-ticks        ← CountsTicks + increment_count

blox crates           ← TOML → codegen (depend on context crates)
  ping-blox, pong-blox, pool-blox, worker-blox, counter-blox
```

### What stays in bloxide-core

Only what *every* blox needs, no exceptions:
- `HasSelfId` + `self_id: ActorId` field pattern (auto-emitted by codegen)
- `ActorId`, `ActorRef`, `BloxRuntime`
- `MachineSpec`, `StateFns`, `StateTopology`
- `StateRule`, `TransitionRule` (transition rules are declared in `blox.toml` via `[[topology.transitions]]` and emitted by `bloxide-codegen`)
- `ActionResult`, `StateRule`

### Service-level crates

Service crates follow the `bloxide-timer` model: the trait, the field pattern, and the action functions all live together. A blox pulls in the crate if it needs that service.

#### `bloxide-messaging`

Provides messaging primitives — references to actor mailboxes. Both `self_ref` and `peer_ref` are `ActorRef<M, R>` where `M` varies per blox. One crate, two traits + action functions:

```rust
// crates/bloxide-messaging/src/lib.rs
use bloxide_core::{BloxRuntime, messaging::ActorRef, ActorId};
use ping_pong_messages::PingPongMsg;

/// Reference to this actor's own mailbox (for self-delivered messages).
pub trait HasSelfRef<R: BloxRuntime, M> {
    fn self_ref(&self) -> &ActorRef<M, R>;
}

/// Reference to a peer actor's mailbox.
pub trait HasPeerRef<R: BloxRuntime, M> {
    fn peer_ref(&self) -> &ActorRef<M, R>;
}

/// Action function: send a Ping message to the peer.
pub fn send_ping<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    round: u32,
) {
    let _ = peer_ref.try_send(self_id, PingPongMsg::Ping(Ping { round }));
}
```

The `BloxCtx` macro auto-generates impls from naming conventions:
- `self_ref: ActorRef<M, R>` → `impl HasSelfRef<R, M>`
- `peer_ref: ActorRef<M, R>` → `impl HasPeerRef<R, M>`

### Domain context crates

Domain context crates own trait definitions + action functions for domain-specific capabilities.

#### Single-field accessor traits

For simple accessor traits (one field, one method), the `BloxCtx` macro auto-generates the impl from the naming convention. The context crate provides only the trait definition:

```rust
// crates/blox-ctx-pool-ref/src/lib.rs
use bloxide_core::{BloxRuntime, messaging::ActorRef};
use pool_messages::PoolMsg;

pub trait HasPoolRef<R: BloxRuntime> {
    fn pool_ref(&self) -> &ActorRef<PoolMsg, R>;
}

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

#### State traits (formerly "delegatable behavior traits")

State traits like `CountsRounds`, `HasCurrentTimer` are now plain traits — no `#[delegatable]` macro, no `B` generic. They are implemented directly on the context struct by the codegen:

```rust
// crates/blox-ctx-rounds/src/lib.rs
pub trait CountsRounds {
    fn round(&self) -> u32;
    fn set_round(&mut self, round: u32);
}

pub fn increment_round(round: &mut u32) {
    *round += 1;
}
```

The codegen generates the impl directly on the context struct:

```rust
// Generated by codegen
impl<R: BloxRuntime> CountsRounds for PingCtx<R> {
    fn round(&self) -> u32 { self.round }
    fn set_round(&mut self, r: u32) { self.round = r; }
}
```

### blox.toml schema

The context section uses `[[context.uses]]` for accessor traits and `[[context.fields]]` for state fields:

```toml
[context]
name = "PingCtx"
generics = "<R: BloxRuntime>"
on_init = "ctx.round = 0; ctx.current_timer = None;"

# Accessor traits — codegen generates trait impls from naming conventions
[[context.uses]]
crate = "bloxide_messaging"
trait = "HasPeerRef<R, PingPongMsg>"
field = "peer_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "accessor"

[[context.uses]]
crate = "bloxide_messaging"
trait = "HasSelfRef<R, PingPongMsg>"
field = "self_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "accessor"

# State fields (formerly in B) — plain fields on the context struct
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
crate = "bloxide_messaging"
kind = "transition"
fields = ["self_id", "peer_ref:ref", "round"]
impl_required = false
```

### Field roles

Each context field has an explicit role that tells the codegen what to emit:

| Role | Codegen behavior |
|------|-----------------|
| `accessor` | Add field, emit trait import, auto-generate accessor impl from naming convention |
| `ctor` | Add field to constructor signature (constructor parameter) |
| `state` (from `[[context.fields]]`) | Add field, zero-initialize in `on_init` |

`self_id` is auto-emitted by the codegen — it is not declared in `blox.toml`.

### What the codegen does with `context.uses`

For each `uses` entry, the codegen:

1. **Adds fields** to the generated struct definition
2. **Emits imports** — `use {crate}::{trait};` for each trait
3. **Emits attributes** — `#[provides(Trait)]` for accessor traits (auto-detected from `_ref` naming)
4. **Emits trait impls** — auto-generated from naming conventions (e.g., `impl HasPeerRef for PingCtx`)

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
- Each context crate shows what traits + fields + action functions it provides
- Set field roles (accessor / ctor / state) via dropdown
- The codegen assembles the struct, imports, and impls

The only hand-written Rust is action function bodies (in context/impl crates) and guard predicate bodies (in `blox.toml` expressions).
