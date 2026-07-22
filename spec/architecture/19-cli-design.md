# `cargo-blox` CLI Design

## Problem Statement

The `cargo-blox` CLI is the primary interface for creating, modifying, and inspecting blox topology. It is used by both humans and AI agents. Today the CLI supports `generate`, `build`, `check`, `test`, `run`, `watch`, `new`, `lint`, `ci`, `verify`, `wire`, `add-state`, `remove-state`, `add-message`, and `remove-message`.

Two gaps exist:

1. **No `add-transition` / `remove-transition`** — the third pillar of topology (after states and messages) requires hand-editing `[[topology.transitions]]` arrays in `blox.toml`.
2. **No `list-*` commands** — the only way to see what exists in a blox is to `cat blox.toml` and parse the entire file. This is error-prone for humans and token-expensive for agents.

Research across actor frameworks (Erlang/OTP, Akka, XState, Boost.SML), CLI-driven config tools (Terraform, Pulumi, Helm, cargo-edit, Rails), and agent-friendly CLI design literature confirms two design decisions:

- **Natural keys, not synthetic IDs.** No system studied uses opaque auto-generated IDs for topology elements. All use natural keys: state+event for transitions (like Boost.SML), name for states (like XState, Erlang), name for messages (like cargo-edit). Terraform's `type.name` composite key is the closest analog to bloxide's `state+event` pair.
- **List commands with `--json` output.** The universal pattern for agent-friendly CLI design is: `list` commands for discoverability, `--json` flag for structured output, semantic exit codes, and idempotent operations. The agent refreshes context by running `list-* --json`, not by re-reading the entire TOML.

## Design

### Design Principles

1. **Natural keys over synthetic IDs.** Every topology element is identified by a stable, human-readable key:
   - States: `name` (unique within a blox)
   - Messages: `variant_name` (unique within a message enum crate)
   - Transitions: `state + event` composite key (unique within a blox)
   - Bloxes: `crate_name` (unique within the workspace)

2. **Full CRUD with list commands.** Every entity type supports `add`, `remove`, and `list`. The `list` command is the discoverability mechanism — it gives agents fresh context without re-reading the entire TOML.

3. **Agent-friendly output.** All `list-*` commands support `--json` for structured output. Human-readable table output is the default. Exit codes are semantic.

4. **Idempotent add.** `add-*` commands detect duplicates and exit with code 5 (conflict) rather than silently creating a second entry. The `--if-not-exists` flag suppresses the error and exits 0 if the entry already exists.

5. **No codegen side effects.** `add-*` / `remove-*` commands mutate only the TOML file. The user runs `cargo blox generate` separately to regenerate code. This matches the existing pattern for `add-state` and `add-message`.

6. **Consistent argument style.** Entity names are positional arguments. Modifiers are `--flag` options. This matches the existing `add-state <blox> <state>` and `add-message <crate> <variant>` patterns.

### Command Inventory

#### Existing commands (unchanged)

| Command | Purpose |
|---------|---------|
| `cargo blox generate` | Run codegen on all `blox.toml` files |
| `cargo blox build` | Generate + `cargo build` |
| `cargo blox check` | Generate + `cargo check` |
| `cargo blox test` | Generate + `cargo test` |
| `cargo blox run` | Generate + `cargo run` |
| `cargo blox watch` | Watch + regenerate on change |
| `cargo blox new <name>` | Scaffold a new blox crate |
| `cargo blox new-actions <name>` | Scaffold a new actions crate |
| `cargo blox new-messages <name>` | Scaffold a new messages crate |
| `cargo blox new-binary <name>` | Scaffold a new wiring binary crate |
| `cargo blox new-all <name>` | Scaffold all layers |
| `cargo blox lint` | Spec-to-code lint checks |
| `cargo blox ci` | Full CI feature matrix |
| `cargo blox verify` | Round-trip: TOML → codegen → viz-export → compare |
| `cargo blox wire` | Generate binary main.rs from system.toml |
| `cargo blox add-state <blox> <state>` | Add a state to a blox topology |
| `cargo blox remove-state <blox> <state>` | Remove a state from a blox topology |
| `cargo blox add-message <crate> <variant> [fields...]` | Add a message variant |
| `cargo blox remove-message <crate> <variant>` | Remove a message variant |

#### New commands

| Command | Purpose |
|---------|---------|
| `cargo blox add-transition` | Add a transition to a blox topology |
| `cargo blox remove-transition` | Remove a transition from a blox topology |
| `cargo blox list-bloxes` | List all blox crates in the workspace |
| `cargo blox list-states <blox>` | List states in a blox |
| `cargo blox list-messages <crate>` | List message variants in a messages crate |
| `cargo blox list-transitions <blox>` | List transitions in a blox |

### New Command Specifications

#### `cargo blox add-transition`

Add a `[[topology.transitions]]` entry to a blox's `blox.toml`.

```
cargo blox add-transition <BLOX_NAME> --state <STATE> --event <EVENT> --target <TARGET>
    [--action <ACTION>]...
    [--guard <CONDITION>:<TARGET>]...
    [--feature <FEATURE>]
    [--if-not-exists]
```

**Arguments:**

| Arg | Required | Description |
|-----|----------|-------------|
| `blox_name` | yes (positional) | Name of the blox crate (e.g. `pool`) |
| `--state` | yes | Source state name (e.g. `Idle`) |
| `--event` | yes | Event pattern (e.g. `PoolMsg::SpawnWorker(_)`) |
| `--target` | yes | Target state, or `stay` / `reset` / `fail` |
| `--action` | no (repeatable) | Action function path (e.g. `Self::handle_spawn_worker`) |
| `--guard` | no (repeatable) | Guard condition and target: `"condition:target"` |
| `--feature` | no | Feature gate (e.g. `dynamic`) |
| `--if-not-exists` | no | Exit 0 if transition already exists |

**Guard syntax:** `--guard "<condition>:<target>"` where condition is a Rust expression and target is a state name. Multiple guards are added in order. Example:

```
--guard "ctx.spawn_in_flight:Spawning" --guard "ctx.pending() == 0:AllDone"
```

**Dedup key:** `state` + `event` pair. If this pair already exists in the blox's transitions, exit code 5 (conflict) unless `--if-not-exists`.

**Output (stderr):** `Added transition <state> + <event> -> <target> to <blox>`

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | Success (or already exists with `--if-not-exists`) |
| 2 | Usage error (missing required arg, blox not found) |
| 5 | Conflict (transition already exists without `--if-not-exists`) |

**TOML output:**

```toml
[[topology.transitions]]
state = "Idle"
event = "PoolMsg::SpawnWorker(_)"
target = "Spawning"
actions = ["handle_spawn_worker"]
feature = "dynamic"
```

With guards:

```toml
[[topology.transitions]]
state = "Spawning"
event = "PoolEvent::SpawnReply(_)"
target = "Active"
actions = ["handle_spawned_worker"]
feature = "dynamic"

[[topology.transitions.guards]]
condition = "ctx.spawn_in_flight || !ctx.spawn_queue.is_empty()"
target = "Spawning"

[[topology.transitions.guards]]
condition = "ctx.pending() == 0 && !ctx.worker_refs().is_empty()"
target = "AllDone"
```

#### `cargo blox remove-transition`

Remove a `[[topology.transitions]]` entry from a blox's `blox.toml`. Also removes any `[[topology.transitions.guards]]` entries nested under it.

```
cargo blox remove-transition <BLOX_NAME> --state <STATE> --event <EVENT>
```

**Arguments:**

| Arg | Required | Description |
|-----|----------|-------------|
| `blox_name` | yes (positional) | Name of the blox crate |
| `--state` | yes | Source state name |
| `--event` | yes | Event pattern |

**Dedup key:** `state` + `event` pair. Matches the exact pair. If not found, exit code 3 (not found).

**Output (stderr):** `Removed transition <state> + <event> from <blox>`

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Usage error (missing required arg, blox not found) |
| 3 | Not found (state+event pair does not exist) |

#### `cargo blox list-bloxes`

List all blox crates in the workspace. Discovers them by scanning for `blox.toml` files under `crates/bloxes/*/`.

```
cargo blox list-bloxes [--json]
```

**Default output (table):**

```
NAME        STATES  TRANSITIONS  MESSAGES
ping        2       6            3
pong        2       1            3
counter     3       3            2
pool        3       9            4
worker      3       3            2
bhsm-tst    8       11           5
```

**JSON output (`--json`):**

```json
[
  {"name": "ping", "states": 2, "transitions": 6, "messages": 3},
  {"name": "pong", "states": 2, "transitions": 1, "messages": 3}
]
```

#### `cargo blox list-states <blox>`

List all states in a blox's topology.

```
cargo blox list-states <BLOX_NAME> [--json]
```

**Default output (table):**

```
NAME      INITIAL  COMPOSITE  TERMINAL  ERROR  PARENT
Idle      yes      no         no        no     —
Active    no       no         no        no     —
AllDone   no       no         yes       no     —
```

**JSON output (`--json`):**

```json
[
  {"name": "Idle", "initial": true, "composite": false, "terminal": false, "error": false, "parent": null},
  {"name": "Active", "initial": false, "composite": false, "terminal": false, "error": false, "parent": null},
  {"name": "AllDone", "initial": false, "composite": false, "terminal": true, "error": false, "parent": null}
]
```

#### `cargo blox list-messages <crate>`

List all message variants in a messages crate's `blox.toml`.

```
cargo blox list-messages <CRATE_NAME> [--json]
```

**Default output (table):**

```
VARIANT   FIELDS
Ping      round: u32
Pong      round: u32
Resume    (none)
```

**JSON output (`--json`):**

```json
[
  {"name": "Ping", "fields": [{"name": "round", "ty": "u32"}]},
  {"name": "Pong", "fields": [{"name": "round", "ty": "u32"}]},
  {"name": "Resume", "fields": []}
]
```

#### `cargo blox list-transitions <blox>`

List all transitions in a blox's topology.

```
cargo blox list-transitions <BLOX_NAME> [--json]
```

**Default output (table):**

```
STATE       EVENT                         TARGET      ACTIONS                         GUARDS  FEATURE
Idle        PoolMsg::SpawnWorker(_)       Spawning    handle_spawn_worker             0       dynamic
Spawning    PoolEvent::SpawnReply(_)      Active      handle_spawned_worker           2       dynamic
Spawning    PoolMsg::SpawnWorker(_)       stay        handle_spawn_worker_queued      0       dynamic
Spawning    PoolMsg::WorkDone(_)          stay        handle_work_done                0       —
Active      PoolMsg::SpawnWorker(_)       Spawning    handle_spawn_worker             0       dynamic
Active      PoolMsg::WorkDone(_)          stay        handle_work_done                1       —
```

**JSON output (`--json`):**

```json
[
  {
    "state": "Idle",
    "event": "PoolMsg::SpawnWorker(_)",
    "target": "Spawning",
    "actions": ["handle_spawn_worker"],
    "guards": [],
    "feature": "dynamic"
  },
  {
    "state": "Spawning",
    "event": "PoolEvent::SpawnReply(_)",
    "target": "Active",
    "actions": ["handle_spawned_worker"],
    "guards": [
      {"condition": "ctx.spawn_in_flight || !ctx.spawn_queue.is_empty()", "target": "Spawning"},
      {"condition": "ctx.pending() == 0 && !ctx.worker_refs().is_empty()", "target": "AllDone"}
    ],
    "feature": "dynamic"
  }
]
```

### Identity Model

| Entity | Natural key | Unique within | Stable? | Self-describing? |
|--------|------------|---------------|---------|-------------------|
| Blox | crate name | workspace | yes | yes |
| State | state name | blox | yes | yes |
| Message | variant name | message enum crate | yes | yes |
| Transition | state + event | blox | yes | yes |

**No synthetic IDs.** The natural key is the identity. This is consistent with every system studied (Erlang, Akka, XState, Boost.SML, Terraform, Pulumi, Helm, cargo-edit, Rails).

**Why not IDs:**
- State+event is already unique and stable (state names become enum variants; event patterns reference message types)
- IDs add TOML noise the codegen must ignore
- IDs require ID generation, gap handling, and renumbering logic
- IDs are inconsistent with existing `add-state`/`add-message` commands (which use name-based matching)
- `list-transitions --json` gives the agent everything an ID would, without the indirection

### Agent-Friendly CLI Properties

| Property | Status | Notes |
|----------|--------|-------|
| Non-interactive | ✅ | All commands accept args, no prompts |
| `--json` output | ✅ (new) | All `list-*` commands |
| List commands | ✅ (new) | `list-bloxes`, `list-states`, `list-messages`, `list-transitions` |
| Idempotent add | ✅ (new) | `--if-not-exists` flag, exit code 5 on conflict |
| Semantic exit codes | ✅ (new) | 0=success, 2=usage, 3=not found, 5=conflict |
| Observable state changes | ✅ (new) | Agent runs `list-*` after add/remove to verify |
| Stable identifiers | ✅ | Natural keys are stable and self-describing |
| Clear error messages | ✅ | Errors include the blox name, state, and event that were not found |

### Exit Code Reference

| Code | Meaning | When |
|------|---------|------|
| 0 | Success | Any successful operation, or `--if-not-exists` and entry already exists |
| 2 | Usage error | Missing required argument, blox crate not found, TOML parse error |
| 3 | Not found | `remove-*` when the natural key does not match any entry |
| 5 | Conflict | `add-*` when the natural key already exists (without `--if-not-exists`) |

### TOML Manipulation Pattern

All `add-*` / `remove-*` commands follow the same internal pattern (established by `state.rs` and `message_cmd.rs`):

1. **Resolve blox.toml path** — `crates/bloxes/<name>/blox.toml` (or `crates/messages/<name>/blox.toml` for messages)
2. **Load TOML** — `toml::from_str` into `toml::Value`
3. **Navigate to target array** — `topology.transitions` for transitions, `topology.states` for states, `messages[i].variants` for messages
4. **Check for duplicate / find entry** — match by natural key
5. **Mutate** — append a new table, or remove the matching table
6. **Save TOML** — `toml::to_string` and write back to file
7. **Print to stderr** — confirmation message

No codegen is triggered. The user runs `cargo blox generate` separately.

### Guard Parsing

Guards are passed as `--guard "<condition>:<target>"` on the CLI. The last `:` separates the condition from the target. This handles conditions containing `::` (e.g. `PoolMsg::WorkDone(_)`):

```
--guard "ctx.spawn_in_flight || !ctx.spawn_queue.is_empty():Spawning"
```

Parses to:
```toml
[[topology.transitions.guards]]
condition = "ctx.spawn_in_flight || !ctx.spawn_queue.is_empty()"
target = "Spawning"
```

**Split on the last `:`** to avoid ambiguity with `::` in Rust paths within the condition expression.

## Test Plan

Tests are unit tests in the `cargo-blox` crate, using a temp directory with a minimal `blox.toml` fixture.

### Fixtures

Each test creates a temp directory with a minimal `blox.toml`:

```toml
[actor]
name = "Test"

[[messages]]
name = "TestMsg"
visibility = "pub"
copy = true

[[messages.variants]]
name = "Ping"

[[messages.variants.fields]]
name = "round"
ty = "u32"

[topology]

[[topology.states]]
name = "Idle"
initial = true

[[topology.states]]
name = "Active"

[[topology.transitions]]
state = "Idle"
event = "TestMsg::Ping(_)"
target = "Active"
```

### Test Cases

#### `add-transition`

| # | Test | Input | Expected |
|---|------|-------|----------|
| 1 | Add basic transition | `--state Idle --event TestMsg::Pong(_) --target Active` | TOML has new `[[topology.transitions]]` with correct fields |
| 2 | Add with actions | `--state Idle --event TestMsg::Pong(_) --target Active --action Self::log --action Self::forward` | TOML has `actions = ["Self::log", "Self::forward"]` |
| 3 | Add with guards | `--state Idle --event TestMsg::Pong(_) --target Active --guard "ctx.x > 0:Active"` | TOML has `[[topology.transitions.guards]]` |
| 4 | Add with feature | `--state Idle --event TestMsg::Pong(_) --target Active --feature dynamic` | TOML has `feature = "dynamic"` |
| 5 | Add with all options | state, event, target, 2 actions, 2 guards, feature | All fields present in TOML |
| 6 | Duplicate rejected | existing state+event pair | Exit code 5 |
| 7 | `--if-not-exists` on duplicate | existing state+event pair | Exit code 0, no change to TOML |
| 8 | Blox not found | non-existent blox name | Exit code 2 |
| 9 | Missing required arg | no `--state` | Exit code 2 |
| 10 | Guard with `::` in condition | `--guard "ctx.msg == TestMsg::Ping(_):Idle"` | Condition parsed correctly (split on last `:`) |

#### `remove-transition`

| # | Test | Input | Expected |
|---|------|-------|----------|
| 1 | Remove existing transition | `--state Idle --event TestMsg::Ping(_)` | TOML no longer has the transition |
| 2 | Remove non-existent | `--state Active --event TestMsg::Ping(_)` | Exit code 3 |
| 3 | Remove with guards | transition that has `[[topology.transitions.guards]]` | Both transition and guards removed |
| 4 | Remove preserves others | blox with multiple transitions | Only the matching one is removed |
| 5 | Blox not found | non-existent blox name | Exit code 2 |

#### `list-transitions`

| # | Test | Input | Expected |
|---|------|-------|----------|
| 1 | List with transitions | blox with 3 transitions | Table shows 3 rows with correct columns |
| 2 | List `--json` | blox with transitions | Valid JSON array with all fields |
| 3 | List empty blox | blox with no transitions | Empty table / `[]` |
| 4 | List with guards | blox with guarded transitions | Guards count shown in table, guard details in JSON |
| 5 | List with features | blox with feature-gated transitions | Feature column shows value |
| 6 | Blox not found | non-existent blox name | Exit code 2 |

#### `list-states`

| # | Test | Input | Expected |
|---|------|-------|----------|
| 1 | List states | blox with 3 states | Table shows name, initial, composite, terminal, error, parent |
| 2 | List `--json` | blox with states | Valid JSON array with all fields |
| 3 | List empty | blox with no states | Empty table / `[]` |
| 4 | Blox not found | non-existent blox name | Exit code 2 |

#### `list-messages`

| # | Test | Input | Expected |
|---|------|-------|----------|
| 1 | List messages | crate with 3 variants | Table shows variant and fields |
| 2 | List `--json` | crate with messages | Valid JSON array with name and fields |
| 3 | List empty | crate with no variants | Empty table / `[]` |
| 4 | Crate not found | non-existent crate name | Exit code 2 |

#### `list-bloxes`

| # | Test | Input | Expected |
|---|------|-------|----------|
| 1 | List bloxes | workspace with 3 bloxes | Table shows name, states, transitions, messages counts |
| 2 | List `--json` | workspace with bloxes | Valid JSON array |
| 3 | Empty workspace | workspace with no bloxes | Empty table / `[]` |

## Implementation Order

1. **`list-*` commands first** — they are read-only, low-risk, and immediately useful for agents. They also establish the TOML-parsing helpers that `add-transition`/`remove-transition` will reuse.
2. **`add-transition`** — builds on the shared helpers, follows the `add-state` pattern
3. **`remove-transition`** — follows the `remove-state` pattern, must handle nested guards

## Invariants

- No `add-*` / `remove-*` command triggers codegen. The user runs `cargo blox generate` separately.
- Natural keys are the sole identity mechanism. No synthetic IDs are added to `blox.toml`.
- `list-*` commands are read-only and never modify the TOML.
- All `list-*` commands support `--json` for agent consumption.
- Exit codes are semantic: 0=success, 2=usage, 3=not found, 5=conflict.
- Guard parsing splits on the **last** `:` to handle `::` in Rust paths.
- Transition matching uses exact string comparison on `state` and `event` fields.
