# Composable Context Crates

## Problem Statement

Today, every blox defines its context struct from scratch in `blox.toml`. When two bloxes need the same capability — e.g., `self_ref: ActorRef<M, R>` — they independently define the field, and the trait (`HasSelfRef`) is duplicated across multiple actions crates. The codegen tries to auto-detect imports by string-matching on field types, which is fragile and produces uncompilable code for generic context types with trait bounds.

The root issues:
1. **Trait definitions are scattered** — `HasPeerRef` lives in `ping_pong_actions`, `HasWorkers` lives in `pool_actions`, `HasSelfId` lives in `bloxide-core::accessor`. No consistent location.
2. **Trait duplication** — `HasSelfRef` exists in both `ping_pong_actions` and `pool_actions` with identical signatures.
3. **Manual impls** — multi-field traits like `HasWorkers` require hand-written `impl` blocks in the blox's `actions.rs`, which gets overwritten on regeneration.
4. **Import inference is broken** — the codegen guesses imports by string-matching field types instead of knowing them declaratively.
5. **No reusability** — a new blox that needs `peer_ref` must depend on `ping_pong_actions` just for the trait, even if it uses completely different action functions.

## Design

### Principle: trait definitions belong with the data, not with the behavior

A context trait is a contract about *what data a context has*. An action function is *what you do with that data*. The contract belongs with the data.

### Three-layer crate model

```
bloxide-core          ← engine (required by all bloxes)
  HasSelfId, ActorId, ActorRef, BloxRuntime, MachineSpec, StateFns, StateRule

service crates        ← infrastructure capabilities (optional)
  bloxide-messaging   ← HasSelfRef<R, M>, HasPeerRef<R, M>
  bloxide-timer       ← HasTimerRef<R>, set_timer, cancel_timer (already exists)

domain context crates ← domain-specific data composition (optional)
  blox-ctx-workers    ← HasWorkers<R>, HasWorkerFactory<R>, WorkerSpawnFn<R>
  blox-ctx-pool-ref   ← HasPoolRef<R>
  blox-ctx-rounds     ← CountsRounds (delegatable)
  blox-ctx-current-timer ← HasCurrentTimer (delegatable)
  blox-ctx-current-task  ← HasCurrentTask (delegatable)
  blox-ctx-worker-peers  ← HasWorkerPeers<R> (delegatable)
  blox-ctx-ticks        ← CountsTicks (delegatable)

actions crates        ← generic functions (depend on context crates for traits)
  ping-pong-actions   ← send_ping, increment_round, etc.
  pool-actions        ← introduce_new_worker, notify_pool_done, etc.
  counter-actions    ← increment_count, etc.

blox crates           ← TOML → codegen (depend on context + actions crates)
  ping-blox, pong-blox, pool-blox, worker-blox, counter-blox
```

### What stays in bloxide-core

Only what *every* blox needs, no exceptions:
- `HasSelfId` + `self_id: ActorId` field pattern
- `ActorId`, `ActorRef`, `BloxRuntime`
- `MachineSpec`, `StateFns`, `StateTopology`
- `StateRule`, `TransitionRule` (transition rules are declared in `blox.toml` via `[[topology.transitions]]` and emitted by `bloxide-codegen`)
- `ActionResult`, `StateRule`

### Service-level crates

Service crates follow the `bloxide-timer` model: the trait, the field pattern, and the action functions all live together. A blox pulls in the crate if it needs that service.

#### `bloxide-messaging` (NEW)

Provides messaging primitives — references to actor mailboxes. Both `self_ref` and `peer_ref` are `ActorRef<M, R>` where `M` varies per blox. One crate, two traits:

```rust
// crates/bloxide-messaging/src/lib.rs
use bloxide_core::{BloxRuntime, messaging::ActorRef};

/// Reference to this actor's own mailbox (for self-delivered messages).
pub trait HasSelfRef<R: BloxRuntime, M> {
    fn self_ref(&self) -> &ActorRef<M, R>;
}

/// Reference to a peer actor's mailbox.
pub trait HasPeerRef<R: BloxRuntime, M> {
    fn peer_ref(&self) -> &ActorRef<M, R>;
}
```

The `BloxCtx` macro auto-generates impls from naming conventions:
- `self_ref: ActorRef<M, R>` → `impl HasSelfRef<R, M>`
- `peer_ref: ActorRef<M, R>` → `impl HasPeerRef<R, M>`

### Domain context crates

Domain context crates own trait definitions + field specs + impl mechanisms for domain-specific capabilities.

#### Single-field accessor traits

For simple accessor traits (one field, one method), the `BloxCtx` macro auto-generates the impl from the naming convention. The context crate provides only the trait definition:

```rust
// crates/blox-ctx-pool-ref/src/lib.rs
use bloxide_core::{BloxRuntime, messaging::ActorRef};
use pool_messages::PoolMsg;

pub trait HasPoolRef<R: BloxRuntime> {
    fn pool_ref(&self) -> &ActorRef<PoolMsg, R>;
}
```

#### Multi-field traits

For traits with multiple methods backed by multiple fields (like `HasWorkers`), the context crate provides a macro that generates the impl for a given context type:

```rust
// crates/blox-ctx-workers/src/lib.rs
use bloxide_core::{BloxRuntime, messaging::ActorRef};
use pool_messages::{WorkerCtrl, WorkerMsg};

pub trait HasWorkers<R: BloxRuntime> {
    fn worker_refs(&self) -> &[ActorRef<WorkerMsg, R>];
    fn worker_refs_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>>;
    fn worker_ctrls(&self) -> &[ActorRef<WorkerCtrl<R>, R>];
    fn worker_ctrls_mut(&mut self) -> &mut Vec<ActorRef<WorkerCtrl<R>, R>>;
    fn pending(&self) -> u32;
    fn set_pending(&mut self, count: u32);
}

pub trait HasWorkerFactory<R: BloxRuntime> {
    fn worker_factory(&self) -> WorkerSpawnFn<R>;
}

#[macro_export]
macro_rules! impl_has_workers {
    ($ctx:ident<$R:ident>) => {
        impl<$R: BloxRuntime> HasWorkers<$R> for $ctx<$R> {
            fn worker_refs(&self) -> &[ActorRef<WorkerMsg, $R>] { &self.worker_refs }
            fn worker_refs_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, $R>> { &mut self.worker_refs }
            fn worker_ctrls(&self) -> &[ActorRef<WorkerCtrl<$R>, $R>] { &self.worker_ctrls }
            fn worker_ctrls_mut(&mut self) -> &mut Vec<ActorRef<WorkerCtrl<$R>, $R>> { &mut self.worker_ctrls }
            fn pending(&self) -> u32 { self.pending }
            fn set_pending(&mut self, count: u32) { self.pending = count; }
        }
    };
}
```

#### Delegatable behavior traits

Behavior traits (like `CountsRounds`, `HasCurrentTimer`) use `#[delegatable]` and are implemented via field delegation (`#[delegates(...)]`). These live in context crates alongside their accessor counterparts:

```rust
// crates/blox-ctx-rounds/src/lib.rs
use bloxide_macros::delegatable;

#[delegatable]
pub trait CountsRounds {
    type Round: Copy + PartialEq + PartialOrd
        + core::ops::Add<Output = Self::Round>
        + From<u8> + core::fmt::Display;
    fn round(&self) -> Self::Round;
    fn set_round(&mut self, round: Self::Round);
}
```

The `#[delegatable]` macro generates the `__delegate_CountsRounds` macro, which the `BloxCtx` derive uses when it sees `#[delegates(CountsRounds)]` on a field.

### blox.toml schema changes

The context section gains a `uses` array for pulling in context crates:

```toml
[context]
name = "PingCtx"
generics = "<R: BloxRuntime, B: HasCurrentTimer + CountsRounds>"
actions_crate = "ping_pong_actions"

# Pull in composable context pieces
[[context.uses]]
crate = "bloxide_messaging"
trait = "HasSelfRef<R, PingPongMsg>"
field = "self_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"          # constructor param, auto-impl via BloxCtx

[[context.uses]]
crate = "bloxide_messaging"
trait = "HasPeerRef<R, PingPongMsg>"
field = "peer_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"          # constructor param, auto-impl via BloxCtx

[[context.uses]]
crate = "bloxide_timer"
trait = "HasTimerRef<R>"
field = "timer_ref"
field_type = "ActorRef<TimerCommand, R>"
role = "ctor"          # constructor param, auto-impl via BloxCtx

[[context.uses]]
crate = "blox_ctx_rounds"
trait = "CountsRounds"
delegatable = true     # used via #[delegates(CountsRounds)]

[[context.uses]]
crate = "blox_ctx_current_timer"
trait = "HasCurrentTimer"
delegatable = true     # used via #[delegates(HasCurrentTimer)]

# This blox's own fields (not from a library)
[[context.fields]]
name = "self_id"
ty = "ActorId"
role = "self_id"       # auto-impl HasSelfId from bloxide-core

[[context.fields]]
name = "behavior"
ty = "B"
role = "delegate"
delegates = ["HasCurrentTimer", "CountsRounds"]
```

For multi-field traits:

```toml
[[context.uses]]
crate = "blox_ctx_workers"
traits = ["HasWorkers<R>", "HasWorkerFactory<R>"]
impl_macro = "impl_has_workers"

  [[context.uses.fields]]
  name = "worker_refs"
  ty = "Vec<ActorRef<WorkerMsg, R>>"
  role = "state"        # zero-initialized

  [[context.uses.fields]]
  name = "worker_ctrls"
  ty = "Vec<ActorRef<WorkerCtrl<R>, R>>"
  role = "state"

  [[context.uses.fields]]
  name = "pending"
  ty = "u32"
  role = "state"

  [[context.uses.fields]]
  name = "worker_factory"
  ty = "WorkerSpawnFn<R>"
  role = "ctor"         # constructor param
```

### Field roles

Each context field has an explicit `role` that tells the codegen what to emit:

| Role | Codegen behavior | BloxCtx behavior |
|------|-----------------|-----------------|
| `self_id` | Add `self_id: ActorId` field | Auto-generate `impl HasSelfId` |
| `accessor` | Add field, emit trait import | Auto-generate accessor impl from naming convention |
| `ctor` | Add field to constructor signature | No auto-impl (use `#[ctor]`) |
| `state` | Add field, zero-initialize | No auto-impl |
| `delegate` | Add field, emit delegate macro imports | `#[delegates(...)]` — delegate to field's impls |

### What the codegen does with `context.uses`

For each `uses` entry, the codegen:

1. **Adds fields** to the generated struct definition
2. **Emits imports** — `use {crate}::{trait};` for each trait, `use {crate}::__delegate_{trait};` for delegatable traits, `use {crate}::{impl_macro};` for multi-field impl macros
3. **Emits attributes** — `#[ctor]` for `role = "ctor"`, `#[delegates(...)]` for delegatable traits, nothing for `role = "state"`
4. **Emits impl macro calls** — `impl_has_workers!(PoolCtx<R>);` after the struct for multi-field traits

The codegen **never guesses imports**. Every import is a direct 1:1 mapping from the TOML.

### spec_skeleton imports

The spec_skeleton gets its own import scope, fully computed from what it references:
- Delegate traits (for `where` bounds on the impl)
- `MachineSpec`, `StateFns`, `PhantomData`, `BloxRuntime`
- The handler table macro (`use crate::{actor}_state_handler_table;`)
- The ctx and event types
- The state enum re-export

It does NOT get context imports (accessor traits, message types, `ActorRef`, etc.) — those are only needed by `ctx.rs`.

### Visual Editor Integration

The blox.toml `[[context.uses]]` entries drive a visual editor where you:
- Add context fields by picking from a library of context crates (dropdown)
- Each context crate shows what traits + fields it provides
- Set field roles (ctor / state / delegate) via dropdown
- The codegen assembles the struct, imports, and impls

The only hand-written Rust is action function bodies and guard predicate bodies, both in actions crates.
