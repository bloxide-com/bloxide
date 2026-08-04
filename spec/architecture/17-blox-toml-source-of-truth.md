# blox.toml as the Single Source of Truth

## Principle

`blox.toml` is the actor's source of truth. The codegen produces Rust source from it.
Generated files are **build artifacts — they are NOT checked into the repository**.
`src/generated/`, `apps/*/src/main.rs`, and `apps/*/Cargo.toml` are all gitignored; only
the TOML files are committed. Generated files are never hand-edited, and no tool treats
them as authoritative.

> **Generate first.** After a fresh checkout (or any TOML edit), the mandatory first
> step is:
>
> ```
> cargo blox generate
> ```
>
> `generate` runs the lint pass first and then rewrites every `src/generated/` and the
> app binaries from the TOML. Without this step there is nothing to compile — the
> generated sources do not exist in a clean clone. CI mirrors this exactly: the
> round-trip job runs `cargo blox generate` before building and testing.

This means:

- Every actor fact that *can* be expressed in TOML *is* expressed in TOML.
- The codegen is deterministic: same TOML → same Rust.
- Hand-written Rust lives only where TOML cannot express intent: action function bodies and complex guard logic.
- The visualizer reads `blox.toml` directly and writes `blox.toml` directly.

## Design

### What `blox.toml` captures

The schema is defined in `crates/tools/bloxide-codegen/src/schema.rs` as `BloxConfig`. Every
struct in the schema carries `#[serde(deny_unknown_fields)]` — **unknown keys are hard
parse errors**, so typos in `blox.toml` fail fast instead of being silently ignored. The
top-level sections are:

| Section | Rust type | Purpose |
|---------|-----------|---------|
| `[actor]` | `ActorConfig` | Actor name (used for state enum, spec struct, event enum). |
| `[[messages]]` | `Vec<MessageEnumConfig>` | Message enums with variants, fields, `Copy`, and visibility. |
| `[event]` | `EventConfig` | Event enum name, generics, `derives` list, feature gates, and mailbox variants. |
| `[topology]` | `TopologyConfig` | States, parent/initial/error flags, declarative transitions, entry/exit actions, and `spec_imports`. |
| `[context]` | `ContextConfig` | Context struct name, generics, fields, imports, `extra_where`, `on_init`, feature gates, `[[context.actions]]`, and `[[context.uses]]` for composable context crates. |
| `[mailboxes]` | `MailboxesConfig` | `max_arity` for generated mailbox tuple impls. |

System-level wiring is a **separate schema** in the same file — `SystemConfig`, parsed
from `system.toml`, not from `blox.toml`. There is no `[wiring]` section in `BloxConfig`.
See `spec/architecture/16-declarative-wiring.md`.

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

The event enum wraps each mailbox as a variant. `message_path` tells the codegen where to import the message type from. `generics` controls the enum declaration, and `derives` is the derive list — an optional list of trait paths that defaults to `["Debug"]` when omitted (an empty list means no derives at all). `feature` / `feature_generics` enable paired `#[cfg]` generation, and each mailbox can carry its own `feature` gate.

#### `[topology]` — states and transitions

From `crates/bloxes/pool/blox.toml`:

```toml
[topology]

[[topology.states]]
name = "Idle"
initial = true

[[topology.states]]
name = "Spawning"

[[topology.states]]
name = "Active"

[[topology.transitions]]
state = "Idle"
event = "PoolMsg::SpawnWorker(_)"
target = "Spawning"
actions = ["Self::handle_spawn_worker"]
feature = "dynamic"

[[topology.transitions]]
state = "Spawning"
event = "PoolEvent::SpawnReply(_)"
target = "Active"
actions = ["Self::handle_spawned_worker"]
feature = "dynamic"

[[topology.transitions.guards]]
condition = "ctx.spawn_in_flight || !ctx.spawn_queue.is_empty()"
target = "Spawning"

[[topology.transitions.guards]]
condition = "ctx.pending == 0 && !ctx.worker_refs.is_empty()"
target = "stop"

[[topology.transitions]]
state = "Active"
event = "PoolMsg::WorkDone(_)"
target = "stay"
actions = ["Self::handle_work_done"]

[[topology.transitions.guards]]
condition = "ctx.pending == 0"
target = "stop"
```

`[topology]` declares:

- The state hierarchy (`parent`, `composite`, `initial`).
- Error flags.
- Declarative transitions with event patterns, action function paths, guards, and targets (`stay`, `reset`, `stop`, `done`, `fail`, or a state name). Actions used in `Self::` form must be declared in `[[context.actions]]`.
- Root-level fallback rules: any `[[topology.transitions]]` entry with the reserved keyword `state = "root"` (`"root"` cannot name a user state). Root rules are evaluated when a domain event bubbles past all user states (the engine-implicit VirtualRoot). Catch-all `event = "_"` is allowed and yields `WILDCARD_TAG`. The codegen emits them as a `ROOT_RULES` constant plus a `root_transitions()` override in the `MachineSpec` impl.
- Per-state `entry` and `exit` action lists.
- `spec_imports` — raw `use` statements for the spec_skeleton module (imports the action functions referenced by transitions/entry/exit).

#### `[context]` — context struct

From `crates/bloxes/ping/blox.toml`:

```toml
[context]
name = "PingCtx"
generics = "<R: BloxRuntime>"
imports = [
    "ping_pong_messages::PingPongMsg",
    "bloxide_timer::{TimerCommand, TimerId}",
]

# self_id is auto-emitted by the codegen as the first field — do NOT declare it.
# Fields come from [[context.uses]] and [[context.fields]] entries:

[[context.uses]]
field = "peer_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

[[context.uses]]
field = "self_ref"
field_type = "ActorRef<PingPongMsg, R>"
role = "ctor"

[[context.uses]]
field = "timer_ref"
field_type = "ActorRef<TimerCommand, R>"
role = "ctor"

[[context.fields]]
name = "current_timer"
type = "Option<TimerId>"

[[context.fields]]
name = "round"
type = "u32"
```

`[context]` declares:

- The struct name and generics.
- `[[context.uses]]` entries that pull fields from composable context crates.
- `[[context.fields]]` entries for state fields on the context struct.
- `self_id: ActorId` (first field) is auto-emitted by the codegen — never declared manually.
- Imports needed by the generated `ctx.rs`.
- `[[context.actions]]` entries declaring each action's signature for the system codegen.

The `role` field tells the codegen how to emit each field. Only two values exist —
anything else (including the retired `accessor` and `self_id` roles) is a **hard
codegen error**, so a typo fails immediately:

| Role | Codegen behavior |
|------|-----------------|
| `ctor` | Adds field to the generated constructor signature. |
| `state` | Adds field; zero-initialized in the generated constructor. |

Notes on `[[context.uses]]` entries:

- `crate` is **optional and informational** — used by scaffolding (`cargo blox
  new-impl`) and the visualizer, not by codegen. The generated imports come from
  `context.imports` plus field-type detection, not from this key.
- A single-field entry uses `field` + `field_type`; a multi-field entry uses
  `[[context.uses.fields]]` with per-field `name` / `ty` / `role`.

Notes on `[[context.actions]]` entries:

- The use site determines the closure signature — an action wired in a
  transition rule gets a transition closure, one wired in `entry`/`exit` gets an
  entry/exit closure. There is no `kind` key.
- `crate` names the crate the function lives in; `impl_required = true` instead
  resolves it from the actor's `impl_crate` in `system.toml`. `fn_name` overrides the
  called function name; `module` inserts a module path segment.
- `fields` lists the context fields the action needs, with access-mode suffixes
  (`field:mut`, `field:ref`, or `field` for copy/owned); `event_payload` /
  `event_arg` control how the event is passed to the function.

#### `[mailboxes]` — mailbox arity

```toml
[mailboxes]
max_arity = 4
```

This controls how many mailbox tuple variants the generated `mailboxes_impls.rs` covers.

#### System wiring — `system.toml` (not part of `BloxConfig`)

Wiring a whole application is described by a separate `system.toml` manifest, parsed
into `SystemConfig` (defined in the same `schema.rs`, also with
`#[serde(deny_unknown_fields)]`). `BloxConfig` has **no `[wiring]` section** — there is
no `WiringConfig`, and no `connections` tables anywhere in the schema.

```toml
# system.toml (from apps/tokio-demo)
[system]
runtime = "tokio"          # "tokio" or "embassy" — anything else is a hard error
name = "tokio-demo"

[[actors]]
name = "ping"
blox = "ping-blox"

  [actors.inject]
  self_ref = { source = "self" }
  peer_ref = { source = "actor", actor = "pong" }
  timer_ref = { source = "actor", actor = "timer" }

[[supervision]]
supervisor = "bloxide-supervisor"
strategy = "when_any_done" # or "when_all_done"; maps to GroupShutdown::WhenAnyDone/WhenAllDone
children = ["ping", "pong"]

  [supervision.policies]
  ping = { stop = true }
  pong = { stop = true }
```

`system_wiring.rs` validates the manifest (in `validate()` and during generation):

1. **Blox refs exist** — every `[[actors]].blox` names a known blox crate.
2. **Inject source actors exist** — every `source = "actor"` reference names a declared
   actor (or the implicit `"supervisor"` when a `[[supervision]]` entry exists).
3. **Inject targets are constructor fields** — an inject entry naming a field that does
   not exist in the blox's constructor is a hard error, *unless* the field exists but is
   feature-gated off (tolerated; the injection is cfg'd out with the field).
4. **Every constructor field has an inject entry** — full coverage is required
   (`self_id` excluded; the codegen fills it).
5. **Secondary mailboxes are bound** — every secondary mailbox in `[event]` needs a
   `source = "self_secondary"` inject entry at its index.
6. **Supervision children are declared** — every `[[supervision]].children` entry names
   a declared actor; unknown `strategy` values are hard errors.

See `spec/architecture/16-declarative-wiring.md` for the full manifest reference.

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
| `ctx.rs` | `[context]` | Context struct with imports and plain fields. |
| `spec_skeleton.rs` | `[actor]` + `[topology]` + `[event]` + `[context]` | `MachineSpec` impl skeleton. |
| `mailboxes_impls.rs` | `[mailboxes]` | Mailbox tuple impls up to `max_arity`. |
| `wiring_main.rs` | `system.toml` | Complete binary `main.rs`. |
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
3. A `<state>_handler_table!` macro that assembles the `HANDLER_TABLE` slice from the per-state `StateFns` associated constants. The `StateFns` constants themselves — raw `StateRule { ... }` struct literals built from `[[topology.transitions]]` entries, plus the `ROOT_RULES` constant from `state = "root"` entries — are emitted by `spec_skeleton.rs`, not `topology.rs`.

From `crates/bloxes/ping/src/generated/topology.rs`:

```rust
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum PingState {
    Operating = 0u8,
    Active = 1u8,
    Paused = 2u8,
    Error = 3u8,
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
            &<$ty>::ERROR_FNS,
        ]
    };
}
```

#### `ctx.rs`

`ctx.rs` emits the context struct with all imports and field attributes, plus a `new()`
constructor whose parameters are exactly the `ctor` fields (`state` fields are
zero-initialized). From `crates/bloxes/ping/src/generated/ctx.rs`:

```rust
use ::bloxide_core::{capability::BloxRuntime, messaging::ActorRef};
use bloxide_timer::{TimerCommand, TimerId};
use ping_pong_messages::PingPongMsg;
pub struct PingCtx<R: BloxRuntime> {
    pub self_id: ::bloxide_core::ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    pub current_timer: Option<TimerId>,
    pub round: u32,
}
impl<R: BloxRuntime> PingCtx<R> {
    pub fn new(
        self_id: ::bloxide_core::ActorId,
        peer_ref: ActorRef<PingPongMsg, R>,
        self_ref: ActorRef<PingPongMsg, R>,
        timer_ref: ActorRef<TimerCommand, R>,
    ) -> Self {
        Self {
            self_id,
            peer_ref,
            self_ref,
            timer_ref,
            current_timer: ::core::default::Default::default(),
            round: ::core::default::Default::default(),
        }
    }
}
```

#### `spec_skeleton.rs`

`spec_skeleton.rs` emits the `MachineSpec` impl plus one `StateFns` constant per state,
built from raw `StateRule { ... }` struct literals. Actions the impl crate will provide
are emitted as **stub closures** marked with `let _stub = "name";` (a marker binding,
not a comment). From `crates/bloxes/ping/src/generated/spec_skeleton.rs` (trimmed):

```rust
pub struct PingSpec<R: BloxRuntime> {
    _phantom: PhantomData<R>,
}
impl<R: BloxRuntime> PingSpec<R> {
    const ACTIVE_FNS: ::bloxide_core::spec::StateFns<Self> = ::bloxide_core::spec::StateFns {
        on_entry: &[|_ctx| {
            let _stub = "send_initial_ping";
        }],
        on_exit: &[],
        transitions: &[::bloxide_core::transition::StateRule {
            event_tag: ::bloxide_core::event_tag::WILDCARD_TAG,
            matches: |__ev| {
                __ev.msg_payload()
                    .is_some_and(|__m| ::core::matches!(__m, PingPongMsg::Pong(_)))
            },
            actions: &[
                |_ctx, _ev| {
                    let _stub = "increment_round";
                    ::bloxide_core::transition::ActionResult::Ok
                },
                |_ctx, _ev| {
                    let _stub = "forward_ping";
                    ::bloxide_core::transition::ActionResult::Ok
                },
            ],
            guard: |ctx, results, _ev| {
                if results.any_failed() {
                    ::bloxide_core::transition::Decision::Transition(
                        ::bloxide_core::topology::LeafState::new(PingState::Error),
                    )
                } else if ctx.round >= MAX_ROUNDS as u32 {
                    ::bloxide_core::transition::Decision::Stop
                } else {
                    ::bloxide_core::transition::Decision::Stay
                }
            },
        }],
    };
    // ... OPERATING_FNS, PAUSED_FNS, ERROR_FNS ...
}
impl<R: BloxRuntime> MachineSpec for PingSpec<R> {
    type State = PingState;
    type Event = PingEvent;
    type Ctx = PingCtx<R>;
    type Mailboxes<Rt: ::bloxide_core::capability::BloxRuntime> =
        (Rt::Stream<ping_pong_messages::PingPongMsg>,);
    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = ping_state_handler_table!(Self);
    fn initial_state() -> PingState {
        PingState::Active
    }
    fn is_error(state: &PingState) -> bool {
        ::core::matches!(state, PingState::Error)
    }
}
```

The blox-crate-level skeleton is a **stub spec** — the real action closures are wired
at the system level, where the codegen regenerates the concrete spec from the same
`blox.toml` plus the `[[context.actions]]` declarations (the `Self::` action
references). See `crates/tools/bloxide-codegen/src/system_spec.rs`.

#### `wiring_main.rs`

From `system.toml`, the codegen emits a complete `main.rs` that creates channels, constructs contexts, builds machines, wires the supervisor tree, and starts the system. See `spec/architecture/16-declarative-wiring.md` for the generated structure and `apps/tokio-pool-demo/src/main.rs` for real output.

### What remains hand-written

Not everything can be expressed in TOML. The following pieces remain hand-written and live *outside* `src/generated/`:

1. **Action function implementations** — the bodies referenced by `topology.transitions[].actions` and `topology.entry/exit[].actions`. These live in context crates (or the app's impl crate). The contract is uniform:
   - **Transition actions** are fallible: `fn(...) -> ActionResult`. The generated
     guard receives the collected `ActionResults` and can react to failures (e.g.
     `results.any_failed()` → error state).
   - **Entry/exit actions** are infallible: `fn(&mut Ctx)`-style functions with no
     return value.
2. **Complex guard logic** — when a guard cannot be expressed as a simple TOML condition string, it is written as a Rust function and referenced from the TOML.
3. **Tests** — `TestRuntime`-based tests in `tests.rs` or inline in `src/lib.rs`.

Until an action is implemented, the codegen's stub closures keep the skeleton
compiling: a stub transition action emits `let _stub = "name";` and returns
`ActionResult::Ok`; a stub entry/exit action emits just `let _stub = "name";`. The
marker is a binding, not a comment, so it survives `cargo fmt` and greps cleanly.

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
  `generate` runs the lint pass first — a lint failure stops the regeneration.
- Generated files carry the header `// Auto-generated by bloxide-codegen. Do not edit manually.`
- Editing generated Rust is forbidden. If a generated file is wrong, fix `blox.toml` or the codegen, not the file.
- Hand-written Rust (actions, tests) is allowed, but it is never placed inside `src/generated/`.

#### `cargo blox` exit codes

`cargo blox` implements semantic process exit codes so scripts and CI can distinguish
failure modes:

| Code | Meaning |
|------|---------|
| 0 | success |
| 1 | unspecified error |
| 2 | usage error (handled natively by clap) |
| 3 | not found (blox, crate, state, variant, actor, …) |
| 5 | conflict (entity already exists) |

Commands construct coded errors (`not_found` / `conflict`); `main` maps them to the
process exit code. See `crates/tools/cargo-blox/src/exit.rs`.

#### Round-trip verification

The round-trip contract is enforced by two automated mechanisms:

1. **Integration tests** (`tools/bloxide-viz-export/tests/round_trip.rs`) — 9 tests that verify every `blox.toml` in the repository can:
   - Be parsed as a `BloxConfig`
   - Produce codegen output without error
   - Be exported by viz-export into a `BloxSpec`
   - Serialize to JSON and deserialize back without data loss
   - Have all states, transitions, and context present in the exported model
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
   - The context struct name matches and auto-emitted fields (`self_id`) are present

Both mechanisms run in CI via the `round-trip-verify` job in `.github/workflows/lint-and-test.yml`.

### UI contract

The visualizer reads `blox.toml` directly (not Rust source):

1. The visualizer loads `blox.toml`, not Rust source.
2. All diagrams, state tables, and wiring graphs are derived from the TOML sections.
3. Edits in the UI write back to `blox.toml`.
4. After writing, the UI triggers `cargo blox generate` to update `src/generated/`.
5. The developer reviews the regenerated Rust and runs tests.
6. The visualizer never writes Rust directly.

This is the vision behind issue #71: a Simulink-like development flow where the actor is built visually from `blox.toml`, regenerated into Rust, and the only hand-written code is action function bodies.

### Validation rules

Validation happens in three places: the TOML parser, the codegen, and
`system_wiring.rs::validate` (for `system.toml`). Current rules:

**TOML parse (serde):**

1. **Unknown keys are hard errors** — every schema struct carries
   `#[serde(deny_unknown_fields)]`, so a misspelled key fails the parse instead of
   being silently dropped.

**Codegen (`blox.toml`):**

2. **State references** — every `topology.transitions[].state`, every transition and
   guard `target`, every `entry`/`exit` state, and every `parent` reference must name a
   declared state (targets may also be `stay`, `reset`, `stop`, `done`, `fail`; `state`
   may also be the reserved keyword `root`, which marks a VirtualRoot fallback rule —
   and no user state may be named `root`). Parent
   chains are checked for cycles. Violations are hard codegen errors.
3. **Pattern and guard syntax** — event patterns and guard conditions must parse as
   Rust syntax; semantic mismatches (e.g. a nonexistent message variant) fail later at
   `cargo build`.
4. **Field roles** — `role` on `[[context.uses]]` entries may only be `"ctor"` or
   `"state"`; anything else is a hard error.
5. **Action declarations** — every `Self::` action referenced by the topology must be
   declared in `[[context.actions]]`; a missing declaration, a missing `crate` on a
   non-`impl_required` action, or `impl_required` without an `impl_crate` in
   `system.toml` is a hard codegen error.

**System wiring (`system.toml`) — `system_wiring.rs` (`validate()` plus generation):**

6. **Blox refs exist** — every `[[actors]].blox` names a known blox crate.
7. **Inject source actors exist** — `source = "actor"` must name a declared actor (or
   the implicit supervisor).
8. **Inject targets are constructor fields** — unknown field names are hard errors
   (except fields that exist but are feature-gated off).
9. **Full inject coverage** — every constructor field must have an inject entry;
   validation is name/coverage only (no type checking — type mismatches fail at
   `cargo build`).
10. **Strategy vocabulary** — `strategy` must be `when_any_done` or `when_all_done`;
    anything else is a hard error.

**Left to the Rust compiler after generation:**

- **Event references** — a transition's event variant must exist on a declared message
  type; the generated `matches!` fails to compile otherwise.
- **Context field types** — `ctx.rs` must compile; undeclared imports or mismatched
  types fail at compile time.
- **Initial state** — exactly one leaf state should be marked `initial = true` (or the
  blox supplies `initial_state()`); mistakes surface as compile or behavior errors.
- **Error state semantics** — `is_error` states report `Failed` to the supervisor;
  actors self-suspend via `Decision::Stop` (no `is_terminal()` — the old terminal state
  model has been removed).

### Extensibility

The TOML schema is designed to be extended deliberately rather than accidentally:

1. **New field roles** — adding a role such as `config` or `metric` only requires a new branch in `ctx.rs` generation; existing roles are unaffected.
2. **New `[[context.uses]]` shapes** — the `ContextUse` struct already supports `field`, `field_type`, `role`, and sub-fields. New optional fields can be added without breaking existing TOML files.
3. **New topology attributes** — optional flags on `StateConfig` (like `composite`, `error`) can be extended with more optional booleans.
4. **Unknown keys are rejected, not ignored** — every schema struct carries
   `#[serde(deny_unknown_fields)]`. Experimental annotations therefore cannot be
   smuggled into `blox.toml`; extending the schema means adding the key to
   `schema.rs` first (usually as an `Option`/defaulted field, which keeps old files
   valid). This is deliberate: typos are caught at parse time instead of silently
   producing wrong code.
5. **New generated file types** — `generate_all` can emit additional files; `mod.rs` is generated from the file list, so new modules are re-exported automatically.

The key is that every extension is opt-in and schema-driven. The codegen does not guess; it reads what the TOML declares.

## Current state vs vision

### What works today

- `blox.toml` is the primary input for `cargo blox generate`.
- The codegen produces `ctx.rs`, `topology.rs`, `spec_skeleton.rs`, `events.rs`, `messages_*.rs`, `mailboxes_impls.rs`, and `wiring_main.rs`.
- Generated files carry the "Do not edit manually" header and are **not committed** —
  `src/generated/`, `apps/*/src/main.rs`, and `apps/*/Cargo.toml` are gitignored, so
  `cargo blox generate` (which runs lint first) is the mandatory first step after
  checkout.
- `cargo blox generate` and `cargo blox watch` regenerate files from TOML.
- `bloxide-viz-export` parses `blox.toml` directly (not Rust source) to produce the visualizer model.
- Round-trip verification is enforced by 9 integration tests and the `cargo blox verify` CLI command, both running in CI.
- Wiring validation (`system_wiring.rs::validate`) checks blox references, inject source actors, inject target names and coverage, secondary-mailbox bindings, and supervision children.

## Visual Editor Integration

The UI is a `blox.toml` (and optionally `system.toml`) editor:

- A state machine canvas that edits `[[topology.states]]` and `[[topology.transitions]]`.
- A message designer that edits `[[messages]]` variants and fields.
- A context panel that edits `[[context.uses]]` entries from a library of composable context crates.
- A wiring canvas that edits `[[actors]]` (with `[actors.inject]`) and `[[supervision]]` in `system.toml`.
- A "Generate" button that runs `cargo blox generate` and reports validation errors.

The only hand-written Rust the UI cannot produce is action function bodies and complex guards — and those live in context crates, not in generated files.

## Related documents

- `spec/architecture/15-composable-context-crates.md` — how `[[context.uses]]` pulls in reusable context crates.
- `spec/architecture/16-declarative-wiring.md` — the `system.toml` wiring manifest and handle injection.
- `spec/architecture/02-hsm-engine.md` — `MachineSpec`, `StateTopology`, and the declarative `[[topology.transitions]]` schema.
- `spec/architecture/05-handler-patterns.md` — transition patterns and guard semantics.
- `spec/architecture/12-action-crate-pattern.md` — the relationship between context crates and bloxes.
