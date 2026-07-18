# Macro Cleanup Plan: One Way to Do Each Task

## Goal
Remove all redundant macros and their fallback paths. Every pattern that can be described as data lives in `blox.toml`. Every pattern that requires Rust code stays as a proc macro. No duplicate paths.

## Phase 1: Migrate Remaining Crates to `blox.toml`

### 1a. `pool-messages` crate
Current: `blox_messages!` twice (PoolMsg, WorkerMsg)

Create `crates/messages/pool-messages/blox.toml`:
```toml
[[messages]]
name = "PoolMsg"
visibility = "pub"

[[messages.variants]]
name = "SpawnWorker"

[[messages.variants.fields]]
name = "task_id"
ty = "u32"

[[messages.variants]]
name = "WorkDone"

[[messages.variants.fields]]
name = "worker_id"
ty = "usize"

[[messages.variants.fields]]
name = "task_id"
ty = "u32"

[[messages.variants.fields]]
name = "result"
ty = "u32"

[[messages]]
name = "WorkerMsg"
visibility = "pub"

[[messages.variants]]
name = "DoWork"

[[messages.variants.fields]]
name = "task_id"
ty = "u32"

[[messages.variants]]
name = "PeerResult"

[[messages.variants.fields]]
name = "from_id"
ty = "usize"

[[messages.variants.fields]]
name = "result"
ty = "u32"
```

Modify `src/lib.rs`:
- Remove `blox_messages!` invocations
- Add `pub mod generated; pub mod prelude; pub use generated::*;`
- Keep manual `WorkerCtrl`, `AddWorkerPeer`, `RemoveWorkerPeer` types (these are generic and can't be generated)

Add `codegen` feature to `Cargo.toml` (default enabled) with fallback.

### 1b. `pool-blox` crate
Current: `event!` and `#[derive(StateTopology)]`

Create `crates/bloxes/pool/blox.toml`:
```toml
[actor]
name = "Pool"

[event]
name = "PoolEvent"

[[event.mailboxes]]
variant = "Msg"
message = "PoolMsg"
message_path = "pool_messages::PoolMsg"

[topology]
handler_fns = ["IDLE_FNS", "ACTIVE_FNS", "ALL_DONE_FNS"]

[[topology.states]]
name = "Idle"

[[topology.states]]
name = "Active"

[[topology.states]]
name = "AllDone"
```

Modify `src/lib.rs`, `src/events.rs`, `src/spec.rs` to use generated modules.
Add `codegen` feature.

### 1c. `worker-blox` crate
Current: `#[blox_event]` and `#[derive(StateTopology)]`

Challenge: `WorkerEvent<R: BloxRuntime>` is generic and `WorkerCtrl<R>` does not implement `Debug`. The `#[blox_event]` attribute macro works because it doesn't derive `Debug`.

Our codegen always derives `Debug` on generated event enums. We need to add a `debug = false` option to `EventConfig`.

Modify `bloxide-codegen/src/schema.rs`:
```rust
#[derive(Debug, Deserialize, Clone)]
pub struct EventConfig {
    pub name: String,
    pub generics: Option<String>, // e.g. "<R: BloxRuntime>"
    pub debug: Option<bool>,      // default true
    pub mailboxes: Vec<MailboxConfig>,
}
```

Modify `bloxide-codegen/src/events.rs`: Only emit `#[derive(Debug)]` if `debug != false`.

Create `crates/bloxes/worker/blox.toml`:
```toml
[actor]
name = "Worker"

[event]
name = "WorkerEvent"
generics = "<R: BloxRuntime>"
debug = false

[[event.mailboxes]]
variant = "Ctrl"
message = "WorkerCtrl"
message_path = "pool_messages::WorkerCtrl"

[[event.mailboxes]]
variant = "Msg"
message = "WorkerMsg"
message_path = "pool_messages::WorkerMsg"

[topology]
handler_fns = ["WAITING_FNS", "DONE_FNS"]

[[topology.states]]
name = "Waiting"

[[topology.states]]
name = "Done"
```

Modify `src/lib.rs`, `src/events.rs`, `src/spec.rs` to use generated modules.
Add `codegen` feature.

### 1d. `bloxide-supervisor` crate (framework crate)
Current: `#[derive(StateTopology)]`, `#[derive(EventTag)]`, hand-written event impls.

The supervisor event is special — variants don't all wrap `Envelope<T>`. We can migrate `StateTopology` to blox.toml but keep the event type hand-written (it's a framework crate, not a user blox).

Create `crates/bloxide-supervisor/blox.toml`:
```toml
[topology]
handler_fns = ["RUNNING_FNS", "SHUTTING_DOWN_FNS"]

[[topology.states]]
name = "Running"

[[topology.states]]
name = "ShuttingDown"
```

Modify `src/supervisor.rs`:
- Replace `#[derive(StateTopology)]` with `pub use crate::generated::topology::SupervisorState;`
- Replace `supervisor_state_handler_table!` with `crate::generated::topology::supervisor_state_handler_table!`
- Keep hand-written `SupervisorEvent` and its impls in `src/event.rs`
- Remove `#[derive(EventTag)]` — the hand-written event already needs `EventTag` impl, which we can write manually or generate separately. Actually, `EventTag` is just sequential numbering. We can write it by hand in 3 lines.

Wait, the supervisor event type has `Child`, `Control`, `Lifecycle` variants. The `EventTag` derive assigns `Child=0, Control=1, Lifecycle=2`. We can replace `#[derive(EventTag)]` with a manual `impl EventTag for SupervisorEvent<R>`.

### 1e. `cargo-blox/src/new.rs` template
Replace `event!` and `#[derive(StateTopology)]` in the template with `blox.toml` generation.
The scaffolded blox crate should have a `blox.toml` that the user edits, and `cargo blox generate` produces the code.

## Phase 2: Remove Fallback Paths from ALL Migrated Crates

For each migrated crate, remove:
- `#[cfg(not(feature = "codegen"))]` blocks and fallback modules
- `messages_fallback.rs`, `events_fallback.rs`, `topology_fallback.rs` files
- `codegen` feature from `Cargo.toml`
- Conditional compilation in `lib.rs` — just `pub mod generated;`

Crates to clean:
- `counter-messages`
- `counter-blox`
- `ping-pong-messages`
- `ping-blox`
- `pong-blox`
- `pool-messages` (after migration)
- `pool-blox` (after migration)
- `worker-blox` (after migration)
- `bloxide-supervisor` (after migration)
- `bloxide-core` (already cleaned — mailboxes_impls removed)

## Phase 3: Remove Unused Proc Macro Implementations

From `crates/bloxide-macros/src/`, remove these files and their exports from `lib.rs`:

| File | Macro | Replaced By |
|---|---|---|
| `blox_messages.rs` | `blox_messages!` | `bloxide-codegen` messages generator |
| `blox_event_new.rs` | `event!` | `bloxide-codegen` events generator |
| `blox_event.rs` | `#[blox_event]` | `bloxide-codegen` events generator (with `debug=false` support) |
| `state_topology.rs` | `#[derive(StateTopology)]` | `bloxide-codegen` topology generator |
| `event_tag.rs` | `#[derive(EventTag)]` | Hand-written or generated via events |
| `mailboxes_impls.rs` | `mailboxes_impls!` | Generated `mailboxes_impls.rs` in bloxide-core |

**KEEP these files (still needed):**
- `transitions.rs` — `transitions!`, `root_transitions!` (contains arbitrary Rust code)
- `blox_ctx/` — `#[derive(BloxCtx)]` (convention-based type inference)
- `delegatable.rs` — `#[delegatable]` (trait AST parsing)
- `channels.rs` — `channels!` (used by `bloxide-embassy` runtime)
- `dyn_channels.rs` — `dyn_channels!` (used by `bloxide-tokio` runtime)
- `next_actor_id!` in `lib.rs` (used by both runtimes)

Also update `bloxide-macros/src/lib.rs` module documentation to only list remaining macros.

## Phase 4: Update Documentation

### `bloxide-macros/src/lib.rs`
Update crate-level documentation. Remove references to removed macros. List only:
- `#[derive(BloxCtx)]`
- `transitions!` / `root_transitions!`
- `#[delegatable]`
- `channels!` (runtime internal)
- `dyn_channels!` (runtime internal)
- `next_actor_id!` (runtime internal)

### `bloxide-core/src/prelude.rs`
Update module doc comment. Remove `#[derive(StateTopology)]` and `#[blox_event]` references.

### `bloxide-core/src/topology.rs`
Update doc comments referencing `#[derive(StateTopology)]` — mention `blox.toml` + `bloxide-codegen` instead.

### `bloxide-core/src/event_tag.rs`
Update doc comments referencing `#[derive(EventTag)]` — mention that `EventTag` is generated by `bloxide-codegen` as part of event generation.

### `skills/building-with-bloxide/SKILL.md`
Major rewrite:
- Layer 1 (Messages): describe `blox.toml` `[[messages]]` tables, not `blox_messages!`
- State Topology: describe `blox.toml` `[topology]` section, not `#[derive(StateTopology)]`
- Event Enum: describe `blox.toml` `[event]` section, not `event!`/`#[blox_event]`
- Context: keep `#[derive(BloxCtx)]` documentation (still needed)
- Transitions: keep `transitions!` documentation (still needed)
- Add a section: "Running `cargo blox generate`" after defining each layer

### `AGENTS.md`
Update the key invariants and where-to-find-things table if it references removed macros.

### `spec/architecture/*.md`
Update any architecture docs that reference the removed macros.

## Phase 5: Update `cargo-blox/src/new.rs` Template

The scaffolding template currently generates:
- `spec.rs` with `#[derive(StateTopology)]` and `transitions!`
- `events.rs` with `event!`

Replace with:
- `blox.toml` containing `[event]` and `[topology]` sections
- `spec.rs` with `pub use crate::generated::topology::*;` and `MachineSpec` impl with `transitions!`
- `events.rs` with `pub use crate::generated::events::*;`

## Phase 6: Verification

After all changes:
1. `cargo check --workspace` must pass with zero warnings
2. `cargo test --workspace` must pass all tests
3. `cargo run -p cargo-blox -- blox generate` must process all `blox.toml` files
4. All examples must compile
5. `cargo check -p bloxide-macros` must still work (reduced but functional)

## Swarm Task Decomposition

### Task A: Migrate remaining blox crates (pool-messages, pool-blox, worker-blox)
Add blox.toml files, modify lib.rs/events.rs/spec.rs, add codegen feature + fallback.
Also enhance codegen schema to support `generics` and `debug=false` on events.

### Task B: Migrate bloxide-supervisor
Add blox.toml with topology only. Replace StateTopology derive with generated module.
Replace EventTag derive with manual impl. Keep hand-written event type.

### Task C: Remove fallback paths from all migrated crates
Remove cfg blocks, fallback modules, and codegen features from:
counter-messages, counter-blox, ping-pong-messages, ping-blox, pong-blox,
pool-messages, pool-blox, worker-blox, bloxide-supervisor, bloxide-core.

### Task D: Remove unused proc macros from bloxide-macros
Delete files, remove exports from lib.rs, update module docs.
Keep transitions, BloxCtx, delegatable, channels, dyn_channels, next_actor_id.

### Task E: Update all documentation
Update SKILL.md, AGENTS.md, bloxide-macros lib.rs docs, bloxide-core docs.
Update cargo-blox new.rs template to use blox.toml instead of macro invocations.

### Task F: Final verification
Run full workspace compile and test. Fix any issues.
