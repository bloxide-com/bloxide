# blox.toml as the Single Source of Truth

## Principle

`blox.toml` is the actor's source of truth. The codegen produces Rust source from it. Generated files are build artifacts — they are checked into the repository for convenience and for `cargo` builds, but they are never hand-edited, and no tool treats them as authoritative.

This means:

- Every actor fact that *can* be expressed in TOML *is* expressed in TOML.
- The codegen is deterministic: same TOML → same Rust.
- Hand-written Rust lives only where TOML cannot express intent: action function bodies, behavior implementations, and complex guard logic.
- The visualizer reads `blox.toml` directly and writes `blox.toml` directly.

## Design

### What `blox.toml` captures

The schema is defined in `crates/tools/bloxide-codegen/src/schema.rs` as `BloxConfig`. The top-level sections are:

| Section | Rust type | Purpose |
|---------|-----------|---------|
| `[actor]` | `ActorConfig` | Actor name (used for state enum, spec struct, event enum). |
| `[[messages]]` | `Vec<MessageEnumConfig>` | Message enums with variants, fields, `Copy`, and visibility. |
| `[event]` | `EventConfig` | Event enum name, generics, `Debug` derive, and mailbox variants. |
| `[topology]` | `TopologyConfig` | States, parent/initial/terminal/error flags, declarative transitions, and entry/exit actions. |
| `[context]` | `ContextConfig` | Context struct name, generics, fields, imports, `extra_where`, `on_init`, and `[[context.uses]]` for composable context crates. |
| `[mailboxes]` | `MailboxesConfig` | `max_arity` for generated mailbox tuple impls. |
| `[wiring]` | `WiringConfig` | Runtime, channels, actor instances, connections, and supervisors for the generated binary. |

#### `[actor]` — actor identity

```toml
[actor]
name = "Ping"
```

This name drives `PingState`, `PingEvent`, `PingCtx`, `PingSpec`, and the generated module prefix.

#### `[[messages]]` — message enums

From `crates/messages/ping-pong-messages/blox.toml`:

```toml
[[messages]]
name = "PingPongMsg"
visibility = "pub"
copy = true

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

Each `[[messages]]` block becomes a standalone Rust enum or struct file (`messages_pingpongmsg.rs`). Variants with fields become named struct variants, satisfying the "named struct variants in message enums" invariant.

#### `[event]` — event enum

```toml
[event]
name = "PingEvent"

[[event.mailboxes]]
variant = "Msg"
message = "PingPongMsg"
message_path = "ping_pong_messages::PingPongMsg"
```

The event enum wraps each mailbox as a variant. `message_path` tells the codegen where to import the message type from. `generics` and `debug` control the enum declaration and derive list.

#### `[topology]` — states and transitions

From `crates/bloxes/pool/blox.toml`:

```toml
[topology]

[[topology.states]]
name = "Idle"
initial = true

[[topology.states]]
name = "Active"

[[topology.states]]
name = "AllDone"
terminal = true

[[topology.transitions]]
state = "Idle"
event = "PoolMsg::SpawnWorker(_)"
target = "Active"
actions = ["handle_spawn_worker"]

[[topology.transitions]]
state = "Active"
event = "PoolMsg::SpawnWorker(_)"
target = "stay"
actions = ["handle_spawn_worker"]

[[topology.transitions]]
state = "Active"
event = "PoolMsg::WorkDone(_)"
target = "stay"
actions = ["handle_work_done"]

[[topology.transitions.guards]]
condition = "ctx.pending() == 0"
target = "AllDone"

[[topology.entry]]
state = "AllDone"
actions = ["log_all_done"]
```

`[topology]` declares:

- The state hierarchy (`parent`, `composite`, `initial`).
- Terminal and error flags.
- Declarative transitions with event patterns, action function paths, guards, and targets (`stay`, `reset`, `fail`, or a state name).
- Per-state `entry` and `exit` action lists.

#### `[context]` — context struct

From `crates/bloxes/ping/blox.toml`:

```toml
[context]
name = "PingCtx"
generics = "<R: BloxRuntime, B: HasCurrentTimer + CountsRounds>"
actions_crate = "ping_pong_actions"
extra_where = ["B: Default", "B::Round: Into<u32>"]
on_init = "ctx.behavior = B::default();"
imports = [
    "ping_pong_actions::{HasPeerRef, HasSelfRef}",
    "ping_pong_messages::PingPongMsg",
    "bloxide_timer::{HasTimerRef, TimerCommand, TimerId}",
]

[[context.fields]]
name = "self_id"
ty = "ActorId"

[[context.fields]]
name = "peer_ref"
ty = "ActorRef<PingPongMsg, R>"

[[context.fields]]
name = "self_ref"
ty = "ActorRef<PingPongMsg, R>"

[[context.fields]]
name = "timer_ref"
ty = "ActorRef<TimerCommand, R>"

[[context.fields]]
name = "behavior"
ty = "B"
delegates = ["HasCurrentTimer", "CountsRounds"]
```

`[context]` declares:

- The struct name and generics.
- Fields with types, roles (`self_id`, `accessor`, `ctor`, `state`, `delegate`), and delegation lists.
- Imports needed by the generated `ctx.rs`.
- `extra_where` predicates appended to the `MachineSpec` impl.
- `on_init` body for `on_init_entry`.
- `actions_crate` for default imports from the actions crate.
- `[[context.uses]]` for composable context crates (see `spec/architecture/15-composable-context-crates.md`).

The `role` field tells the codegen how to emit each field:

| Role | Codegen behavior |
|------|-----------------|
| `self_id` | Adds `self_id: ActorId`; auto-impls `HasSelfId`. |
| `accessor` | Adds field; auto-impls accessor trait from naming convention. |
| `ctor` | Adds field to the `BloxCtx`-generated constructor signature. |
| `state` | Adds field; zero-initialized in the generated constructor. |
| `delegate` | Adds `#[delegates(...)]` attribute on a behavior field. |

#### `[mailboxes]` — mailbox arity

```toml
[mailboxes]
max_arity = 4
```

This controls how many mailbox tuple variants the generated `mailboxes_impls.rs` covers.

#### `[wiring]` — generated binary

`[wiring]` is the in-spec wiring section. For real applications, use the separate `system.toml` manifest described in `spec/architecture/16-declarative-wiring.md`. Both are parsed from the same `WiringConfig` / `SystemConfig` schema.

```toml
[wiring]
runtime = "tokio"

[[wiring.actors]]
blox = "ping"
name = "ping"
behavior = "DemoBehavior"
behavior_traits = ["CountsRounds", "HasCurrentTimer"]

  [wiring.actors.context_fields]
  peer_ref = "pong"
  timer_ref = "timer"

[[wiring.actors]]
blox = "pong"
name = "pong"

[[wiring.connections]]
from = "ping"
to = "pong"
message = "PingPongMsg"
channel_capacity = 16

[[wiring.supervisors]]
name = "sup"
strategy = "one_for_one"

  [[wiring.supervisors.children]]
  actor = "ping"
  restart_max = 1
```

### What the codegen generates

`crates/tools/bloxide-codegen/src/lib.rs::generate_all` produces a set of files, all prefixed with the same header:

```rust
// Auto-generated by bloxide-codegen. Do not edit manually.
```

| Generated file | Source section | Contents |
|----------------|----------------|----------|
| `messages_<name>.rs` | `[[messages]]` | Enum + struct variants for one message type. |
| `events.rs` | `[event]` | Event enum wrapping mailbox variants, plus trait impls. |
| `topology.rs` | `[topology]` + `[actor]` | State enum, `StateTopology` impl, and `StateFns` constants or handler table. |
| `ctx.rs` | `[context]` | Context struct with `#[derive(BloxCtx)]`, imports, and field attributes. |
| `spec_skeleton.rs` | `[actor]` + `[topology]` + `[event]` + `[context]` | `MachineSpec` impl skeleton. |
| `mailboxes_impls.rs` | `[mailboxes]` | Mailbox tuple impls up to `max_arity`. |
| `wiring_main.rs` | `[wiring]` or `system.toml` | Complete binary `main.rs`. |
| `mod.rs` | All of the above | Re-exports every generated submodule. |

#### `messages_<name>.rs`

For each `[[messages]]` entry the codegen emits a Rust enum with named struct variants. If `copy = true`, the enum derives `Copy` in addition to `Debug` and `Clone`.

#### `events.rs`

The event enum combines all declared mailboxes. For `PingEvent` with one mailbox variant `Msg(PingPongMsg)`, the generated enum looks like:

```rust
pub enum PingEvent {
    Msg(PingPongMsg),
}
```

It also emits `From` impls and `Debug` when requested.

#### `topology.rs`

`topology.rs` emits:

1. A `#[repr(u8)]` state enum with one variant per `[[topology.states]]`.
2. A `StateTopology` impl with `parent`, `is_leaf`, `path`, and `as_index`.
3. Complete `StateFns` constants built from raw `StateRule { ... }` struct literals emitted by `bloxide-codegen` from `[[topology.transitions]]` entries. Codegen emits the arrays directly.

From `crates/bloxes/ping/src/generated/topology.rs`:

```rust
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum PingState {
    Operating = 0u8,
    Active = 1u8,
    Paused = 2u8,
    Done = 3u8,
    Error = 4u8,
}

impl ::bloxide_core::topology::StateTopology for PingState { /* ... */ }

#[doc(hidden)]
#[macro_export]
macro_rules! ping_state_handler_table {
    ($ty:ty) => {
        &[
            &<$ty>::OPERATING_FNS,
            &<$ty>::ACTIVE_FNS,
            &<$ty>::PAUSED_FNS,
            &<$ty>::DONE_FNS,
            &<$ty>::ERROR_FNS,
        ]
    };
}
```

#### `ctx.rs`

`ctx.rs` emits the context struct with all imports and field attributes. From `crates/bloxes/ping/src/generated/ctx.rs`:

```rust
use ping_pong_actions::{HasCurrentTimer, CountsRounds, __delegate_HasCurrentTimer, __delegate_CountsRounds};
use ping_pong_actions::{HasPeerRef, HasSelfRef};
use ping_pong_messages::PingPongMsg;
use bloxide_timer::{HasTimerRef, TimerCommand, TimerId};
use ::bloxide_core::{capability::BloxRuntime, messaging::ActorRef};
use ::bloxide_core::ActorId;
use ::bloxide_macros::BloxCtx;

#[derive(BloxCtx)]
pub struct PingCtx<R: BloxRuntime, B: HasCurrentTimer + CountsRounds> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
```

#### `spec_skeleton.rs`

`spec_skeleton.rs` emits the `MachineSpec` impl. It references the state enum, event enum, context type, and mailbox tuple type. From `crates/bloxes/ping/src/generated/spec_skeleton.rs`:

```rust
use ::core::marker::PhantomData;
use ::bloxide_core::capability::BloxRuntime;
use ::bloxide_core::spec::{MachineSpec, StateFns};
use crate::{PingCtx, PingEvent};
pub use crate::generated::topology::PingState;
use ping_pong_actions::{HasCurrentTimer, CountsRounds};
use ping_pong_actions::{HasPeerRef, HasSelfRef};
use ping_pong_messages::PingPongMsg;
use bloxide_timer::{HasTimerRef, TimerCommand, TimerId};

pub struct PingSpec<R: BloxRuntime, B: HasCurrentTimer + CountsRounds + 'static>
where
    B: Default,
    B::Round: Into<u32>,
{
    _phantom: PhantomData<(R, B)>,
}

impl<R: BloxRuntime, B: HasCurrentTimer + CountsRounds + 'static> MachineSpec for PingSpec<R, B>
where
    B: Default,
    B::Round: Into<u32>,
{
    type State = PingState;
    type Event = PingEvent;
    type Ctx = PingCtx<R, B>;
    type Mailboxes<Rt: ::bloxide_core::capability::BloxRuntime> = (
        Rt::Stream<ping_pong_messages::PingPongMsg>,
    );
    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = ping_state_handler_table!(Self);
    fn initial_state() -> PingState { PingState::Active }
    fn is_terminal(state: &PingState) -> bool { ::core::matches!(state, PingState::Done) }
    fn is_error(state: &PingState) -> bool { ::core::matches!(state, PingState::Error) }
    fn on_init_entry(ctx: &mut Self::Ctx) { ctx.behavior = B::default(); }
}
```

#### `wiring_main.rs`

From `[wiring]` or `system.toml`, the codegen emits a complete `main.rs` that creates channels, constructs contexts, builds machines, wires the supervisor tree, and starts the system. See `spec/architecture/16-declarative-wiring.md` for the generated structure.

### What remains hand-written

Not everything can be expressed in TOML. The following pieces remain hand-written and live *outside* `src/generated/`:

1. **Action function implementations** — the bodies referenced by `topology.transitions[].actions` and `topology.entry/exit[].actions`. These live in `*-actions` crates.
2. **Behavior implementations** — concrete types that implement delegatable traits such as `CountsRounds` or `HasCurrentTimer`. These live in `*-impl` crates.
3. **Complex guard logic** — when a guard cannot be expressed as a simple TOML condition string, it is written as a Rust function and referenced from the TOML.
4. **Tests** — `TestRuntime`-based tests in `tests.rs` or inline in `src/lib.rs`.

The rule is: if it is in `src/generated/`, it is produced by `cargo blox generate`. If it is anywhere else, it is hand-written and preserved across regeneration.

### Round-trip contract

The round-trip contract is the core of this design:

```
Edit blox.toml
      ↓
cargo blox generate
      ↓
Updated Rust code in src/generated/
      ↑
Never edit generated files by hand
```

- `blox.toml` is the only editable spec.
- `cargo blox generate` (and `cargo blox watch`) rewrites `src/generated/` from the TOML.
- Generated files carry the header `// Auto-generated by bloxide-codegen. Do not edit manually.`
- Editing generated Rust is forbidden. If a generated file is wrong, fix `blox.toml` or the codegen, not the file.
- Hand-written Rust (actions, behaviors, tests) is allowed, but it is never placed inside `src/generated/`.

#### Round-trip verification

The round-trip contract is enforced by two automated mechanisms:

1. **Integration tests** (`tools/bloxide-viz-export/tests/round_trip.rs`) — 10 tests that verify every `blox.toml` in the repository can:
   - Be parsed as a `BloxConfig`
   - Produce codegen output without error
   - Be exported by viz-export into a `BloxSpec`
   - Serialize to JSON and deserialize back without data loss
   - Have all states, transitions, context, and wiring present in the exported model
   - Round-trip back to the original `BloxConfig` fields with no data loss
   - Produce deterministic codegen output (same input → same output)

2. **`cargo blox verify` CLI command** — a standalone verification command that runs the full pipeline (blox.toml → codegen → viz-export → JSON → compare) and reports any data loss or missing fields. This can be run locally before pushing and is also run in CI.

   ```
   cargo blox verify
   cargo blox verify --workspace /path/to/workspace
   ```

   The command checks:
   - Every `blox.toml` parses successfully
   - Codegen produces output for every actor blox
   - viz-export produces a `BloxSpec` for every actor
   - JSON serialization round-trips with no data loss
   - All states from the TOML are present in the exported spec
   - All declarative transitions are present as explicit handlers
   - Context struct name and fields match
   - Wiring runtime, actors, and connections match

Both mechanisms run in CI via the `round-trip-verify` job in `.github/workflows/lint-and-test.yml`.

### UI contract

The visualizer reads `blox.toml` directly (not Rust source):

1. The visualizer loads `blox.toml`, not Rust source.
2. All diagrams, state tables, and wiring graphs are derived from the TOML sections.
3. Edits in the UI write back to `blox.toml`.
4. After writing, the UI triggers `cargo blox generate` to update `src/generated/`.
5. The developer reviews the regenerated Rust and runs tests.
6. The visualizer never writes Rust directly.

This is the vision behind issue #71: a Simulink-like development flow where the actor is built visually from `blox.toml`, regenerated into Rust, and the only hand-written code is action function bodies and behavior implementations.

### Validation rules

Validation happens in two places: the codegen parser and the `wiring::validate` function. Current rules from `crates/tools/bloxide-codegen/src/wiring.rs::validate` include:

1. **Wiring actors must have non-empty blox names** — every `[[wiring.actors]]` entry must reference a real blox crate.
2. **Connection endpoints must be declared actors** — every `connections.from` and `connections.to` must match an actor name in `[[wiring.actors]]`.
3. **Context field references must be declared actors** — every key/value in `wiring.actors[].context_fields` must reference an actor declared in `[[wiring.actors]]`.
4. **Supervisor children must be declared actors** — every `wiring.supervisors[].children[].actor` must exist in `[[wiring.actors]]`.

Additional validation that should be enforced (some by the Rust compiler after generation, some by the codegen):

5. **State references** — every `target` in `topology.transitions` and `topology.transitions.guards` must name a declared state, or one of `stay`, `reset`, `fail`.
6. **Event references** — every `topology.transitions[].event` must match a variant of a declared message type.
7. **Context field types** — `ctx.rs` must compile; undeclared imports or mismatched types fail at compile time.
8. **Wiring consistency** — injected constructor params must match the context field types; message types on connections must match the receiving actor's mailbox.
9. **Initial state** — exactly one leaf state must be marked `initial = true` (or the `initial_state()` function must be supplied).
10. **Terminal/error exclusivity** — `is_error` takes precedence over `is_terminal`; states should not be both unless the failure semantics are intentional.

### Extensibility

The TOML schema is designed to be extended without breaking existing codegen:

1. **New field roles** — adding a role such as `config` or `metric` only requires a new branch in `ctx.rs` generation; existing roles are unaffected.
2. **New `[[context.uses]]` shapes** — the `ContextUse` struct already supports `trait`, `traits`, `field`, `field_type`, `role`, `delegatable`, `impl_macro`, and sub-fields. New optional fields can be added without invalidating old TOML.
3. **New topology attributes** — optional flags on `StateConfig` (like `composite`, `terminal`, `error`) can be extended with more optional booleans.
4. **Custom annotations** — unknown keys in TOML are ignored by serde by default, so experimental annotations can be added to `blox.toml` and consumed by future codegen versions or UI tools without breaking current builds.
5. **New generated file types** — `generate_all` can emit additional files; `mod.rs` is generated from the file list, so new modules are re-exported automatically.

The key is that every extension is opt-in and schema-driven. The codegen does not guess; it reads what the TOML declares.

## Current state vs vision

### What works today

- `blox.toml` is the primary input for `cargo blox generate`.
- The codegen produces `ctx.rs`, `topology.rs`, `spec_skeleton.rs`, `events.rs`, `messages_*.rs`, `mailboxes_impls.rs`, and `wiring_main.rs`.
- Generated files carry the "Do not edit manually" header.
- `cargo blox generate` and `cargo blox watch` regenerate files from TOML.
- `bloxide-viz-export` parses `blox.toml` directly (not Rust source) to produce the visualizer model.
- Round-trip verification is enforced by 10 integration tests and the `cargo blox verify` CLI command, both running in CI.
- Wiring validation checks actor references, connection endpoints, context field references, and supervisor children.

## Visual Editor Integration

The UI is a `blox.toml` (and optionally `system.toml`) editor:

- A state machine canvas that edits `[[topology.states]]` and `[[topology.transitions]]`.
- A message designer that edits `[[messages]]` variants and fields.
- A context panel that edits `[[context.fields]]` and `[[context.uses]]` from a library of composable context crates.
- A wiring canvas that edits `[[wiring.actors]]`, `[[wiring.connections]]`, and `[[wiring.supervisors]]` (or the equivalent `system.toml` tables).
- A "Generate" button that runs `cargo blox generate` and reports validation errors.

The only hand-written Rust the UI cannot produce is action function bodies, behavior implementations, and complex guards — and those live in actions/impl crates, not in generated files.

## Related documents

- `spec/architecture/15-composable-context-crates.md` — how `[[context.uses]]` pulls in reusable context crates.
- `spec/architecture/16-declarative-wiring.md` — the `system.toml` wiring manifest and handle injection.
- `spec/architecture/02-hsm-engine.md` — `MachineSpec`, `StateTopology`, and the declarative `[[topology.transitions]]` schema.
- `spec/architecture/05-handler-patterns.md` — transition patterns and guard semantics.
- `spec/architecture/12-action-crate-pattern.md` — the relationship between actions crates, impl crates, and bloxes.
