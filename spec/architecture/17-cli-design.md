# `cargo-blox` CLI Design

## Problem Statement

The `cargo-blox` CLI is the primary interface for creating, modifying, and inspecting blox topology and app wiring. It is used by both humans and AI agents. It covers four areas:

- **Codegen and cargo loops** — `generate`, `build`, `check`, `test`, `run`, `watch`.
- **Scaffolding** — `new`, `new-messages`, `new-context`, `new-impl`, `new-binary`, `new-all`, `init`.
- **Declarative edits** — `add-*` / `remove-*` / `set-policy` commands that mutate `blox.toml` (states, transitions, entry/exit hooks, messages, context declarations) and `system.toml` (actors, supervision, policies, injections).
- **Inspection and tooling** — `list-bloxes`, `list-states`, `list-transitions`, `list-messages`, plus `lint`, `ci`, `verify`, `wire`, `viz`.

Research across actor frameworks (Erlang/OTP, Akka, XState, Boost.SML), CLI-driven config tools (Terraform, Pulumi, Helm, cargo-edit, Rails), and agent-friendly CLI design literature confirms two design decisions the CLI follows:

- **Natural keys, not synthetic IDs.** No system studied uses opaque auto-generated IDs for topology elements. All use natural keys: state+event for transitions (like Boost.SML), name for states (like XState, Erlang), name for messages (like cargo-edit). Terraform's `type.name` composite key is the closest analog to bloxide's `state+event` pair.
- **List commands with `--json` output.** The universal pattern for agent-friendly CLI design is: `list` commands for discoverability, `--json` flag for structured output, semantic exit codes, and idempotent operations. The agent refreshes context by running `list-* --json`, not by re-reading the entire TOML.

## Design

### Design Principles

1. **Natural keys over synthetic IDs.** Every topology element is identified by a stable, human-readable key:
   - States: `name` (unique within a blox)
   - Messages: `variant_name` (unique within a message enum crate)
   - Transitions: `state + event` composite key (unique within a blox)
   - Bloxes: `crate_name` (unique within the workspace)
   - Actors: `name` (unique within a `system.toml`)

2. **Full CRUD with list commands.** Every entity type supports `add`, `remove`, and `list` where listing makes sense. The `list` command is the discoverability mechanism — it gives agents fresh context without re-reading the entire TOML.

3. **Agent-friendly output.** All `list-*` commands support `--json` for structured output. Human-readable table output is the default. Exit codes are semantic.

4. **Idempotent add.** `add-*` commands detect duplicates and exit with code 5 (conflict) rather than silently creating a second entry. The `--if-not-exists` flag (available on `add-transition`, `add-entry`, `add-exit`, `add-actor`, `add-supervision`, `add-use`, `add-field`, `add-action`) suppresses the error and exits 0 silently if the entry already exists.

5. **No codegen side effects.** `add-*` / `remove-*` commands mutate only the TOML file. The user runs `cargo blox generate` separately to regenerate code (the `system.toml` commands print a reminder to that effect on success).

6. **Consistent argument style.** Entity names are positional arguments. Modifiers are `--flag` options. This matches the `add-state <blox> <state>` and `add-message <crate> <variant>` patterns.

7. **Comment-preserving edits.** All TOML mutation goes through `toml_edit::DocumentMut`, so hand-written comments and formatting survive CLI edits (see *TOML Manipulation Convention*).

### Command Inventory

#### Codegen and cargo loops

| Command | Purpose |
|---------|---------|
| `cargo blox generate [--workspace <dir>]` | Lint, then regenerate all blox crates and app wiring |
| `cargo blox build [cargo-args...]` | Generate + `cargo build` |
| `cargo blox check [cargo-args...]` | Generate + `cargo check` |
| `cargo blox test [cargo-args...]` | Generate + `cargo test` |
| `cargo blox run [cargo-args...]` | Generate + `cargo run` |
| `cargo blox watch` | Regenerate + `cargo check` on blox.toml/system.toml changes |

`build` / `check` / `test` / `run` accept the standard cargo feature flags (`--features`, `--no-default-features`, `--all-features` via `clap_cargo::Features`) and forward any trailing arguments to cargo. `watch` accepts the cargo feature flags.

#### Scaffolding

| Command | Purpose |
|---------|---------|
| `cargo blox new <name> [--messages <crate>] [--context <crate>]` | Scaffold a blox crate + `spec/bloxes/<name>.md` |
| `cargo blox new-messages <name>` | Scaffold a messages crate (`crates/messages/<name>-messages/`) |
| `cargo blox new-context <name>` | Scaffold a context crate (`crates/context/blox-ctx-<name>/`) |
| `cargo blox new-impl <name> --blox <blox>` | Scaffold an impl crate from the blox's `impl_required` actions |
| `cargo blox new-binary <name> [--runtime <tokio\|embassy>]` | Scaffold an app: `apps/<name>/system.toml` (main.rs and Cargo.toml are generated) |
| `cargo blox new-all <name> [--runtime <tokio\|embassy>]` | Scaffold all layers (messages, context, blox, impl, app), then generate |
| `cargo blox init <dir> [--runtime <tokio\|embassy>]` | Bootstrap a new bloxide workspace |

All scaffolding commands register new crates in the workspace `Cargo.toml` (members + dependencies). `--runtime` defaults to `tokio`.

#### Blox topology edits (`crates/bloxes/<blox>/blox.toml`)

| Command | Purpose |
|---------|---------|
| `cargo blox add-state <blox> <state> [--parent <state>] [--composite] [--error]` | Add a state |
| `cargo blox remove-state <blox> <state>` | Remove a state |
| `cargo blox add-transition <blox> --state <s> --event <e> --target <t> [--action <path>]... [--guard <cond>:<target>]... [--feature <f>] [--if-not-exists]` | Add a transition |
| `cargo blox remove-transition <blox> --state <s> --event <e>` | Remove a transition |
| `cargo blox add-entry <blox> --state <s> [--action <path>]... [--feature <f>] [--if-not-exists]` | Add an entry hook |
| `cargo blox remove-entry <blox> --state <s>` | Remove an entry hook |
| `cargo blox add-exit <blox> --state <s> [--action <path>]... [--feature <f>] [--if-not-exists]` | Add an exit hook |
| `cargo blox remove-exit <blox> --state <s>` | Remove an exit hook |

#### Message edits (`crates/messages/<crate>/blox.toml`)

| Command | Purpose |
|---------|---------|
| `cargo blox add-message <crate> <variant> [name:ty ...]` | Add a message variant |
| `cargo blox remove-message <crate> <variant>` | Remove a message variant |

#### Context edits (`crates/bloxes/<blox>/blox.toml` `[context]` section)

| Command | Purpose |
|---------|---------|
| `cargo blox add-use <blox> --field <f> --field-type <ty> --role <ctor\|state> [--feature <f>] [--if-not-exists]` | Add a single-field `[[context.uses]]` entry |
| `cargo blox add-use <blox> --sub-field <name:ty:role>... [--feature <f>] [--if-not-exists]` | Add a multi-field `[[context.uses]]` entry |
| `cargo blox remove-use <blox> --field <f>` | Remove a `[[context.uses]]` entry (or a sub-field from a multi-field entry) |
| `cargo blox add-field <blox> --name <n> --ty <ty> [--default <expr>] [--if-not-exists]` | Add a `[[context.fields]]` state field |
| `cargo blox remove-field <blox> --name <n>` | Remove a context field (from `fields`, `uses`, or `uses.fields`) |
| `cargo blox add-action <blox> --name <n> [--field <f>]... [--crate-name <c>] [--module <m>] [--fn-name <f>] [--event-payload <ty>] [--impl-required] [--returns ActionResult] [--feature <f>] [--if-not-exists]` | Add a `[[context.actions]]` entry |
| `cargo blox remove-action <blox> --name <n>` | Remove a `[[context.actions]]` entry |

#### System wiring edits (`apps/<app>/system.toml`)

| Command | Purpose |
|---------|---------|
| `cargo blox add-actor <app> --name <n> --blox <crate> [--impl-crate <c>] [--kind <k>] [--feature <f>]... [--if-not-exists]` | Add an `[[actors]]` entry |
| `cargo blox remove-actor <app> --name <n>` | Remove an actor (also cleans supervision refs) |
| `cargo blox add-supervision <app> --supervisor <n> --strategy <when_any_done\|when_all_done> [--child <actor>]... [--if-not-exists]` | Add a `[[supervision]]` group |
| `cargo blox remove-supervision <app> --supervisor <n>` | Remove a supervision group |
| `cargo blox set-policy <app> --actor <n> [--restart-max <n>] [--stop]` | Set a child policy in a supervision group |
| `cargo blox add-injection <app> --actor <n> --field <f> --from <source>` | Add a constructor injection to an actor |

#### Inspection

| Command | Purpose |
|---------|---------|
| `cargo blox list-bloxes [--json]` | List all blox crates in the workspace |
| `cargo blox list-states <blox> [--json]` | List states in a blox |
| `cargo blox list-transitions <blox> [--json]` | List transitions in a blox |
| `cargo blox list-messages <crate> [--json]` | List message variants in a messages crate |

#### Tooling

| Command | Purpose |
|---------|---------|
| `cargo blox lint` | Friendly TOML validation with did-you-mean suggestions |
| `cargo blox ci` | Full CI feature matrix |
| `cargo blox verify [--workspace <dir>]` | Round-trip: TOML → codegen → viz-export → JSON → compare |
| `cargo blox wire [--system <path>] [--output <path>] [--run]` | Generate a main.rs from one system.toml manifest |
| `cargo blox viz [--export <dir>] [--port <n>] [--open]` | Launch the visualizer (or export specs as JSON) |

### Command Specifications

#### `cargo blox generate`

Runs lint first (issues #114/#122): invalid TOML fails fast with friendly diagnostics instead of surfacing as codegen errors or Rust compile errors downstream.

Then, for every `blox.toml` in the workspace (walk skipping `target/`):

1. Runs `bloxide_codegen::generate_from_toml` and writes the files into the crate's `src/generated/`.
2. Each generated file is formatted individually with `rustfmt --edition 2021` (best effort — an unavailable or failing rustfmt leaves the content unformatted). There is deliberately **no** workspace-wide `cargo fmt`, which would also rewrite hand-written files.
3. Files are written **only when their content changed** — re-running `generate` with no changes prints no `generated ...` lines and preserves mtimes for cargo caching. `src/generated/mod.rs` is owned by the CLI (single writer) and lists exactly the files generated this run.

Then, for every `system.toml` in the workspace, it regenerates the app's `src/main.rs` (system wiring) and `Cargo.toml` (dependencies), with the same write-only-if-changed behavior.

`--workspace <dir>` overrides root discovery; by default the root is found by walking up from `CARGO_MANIFEST_DIR`.

#### `cargo blox watch`

Watches the workspace recursively (ignoring `target/`) for `blox.toml` and `system.toml` changes, debounced at 500 ms. On each change it regenerates and runs `cargo check` with the given feature flags. The watched root is found by walking up from the current directory (falling back to the current directory outside a workspace) — never `CARGO_MANIFEST_DIR`, which points at the cargo-blox crate when the binary runs under `cargo run`.

#### Scaffolding commands

- **`new <name>`** — creates `crates/bloxes/<name>/` (Cargo.toml, src, blox.toml) and a spec skeleton `spec/bloxes/<name>.md` from `spec/templates/blox-spec.md`. `--messages` / `--context` wire the named dependency crates into the new blox crate.
- **`new-messages <name>`** — creates `crates/messages/<name>-messages/` with a stub `XxxMsg` enum in blox.toml.
- **`new-context <name>`** — creates `crates/context/blox-ctx-<name>/` (free action functions, `#![no_std]`, no traits).
- **`new-impl <name> --blox <blox>`** — reads the blox's blox.toml, finds `[[context.actions]]` entries with `impl_required = true`, and creates `crates/impl/<name>/` with matching function stubs. Missing blox.toml → exit 3.
- **`new-binary <name>`** — writes only `apps/<name>/system.toml`; the app's `Cargo.toml` and `src/main.rs` are generated by `cargo blox generate` (system.toml is the single source of truth for wiring).
- **`new-all <name>`** — runs the five scaffolds in order (messages → context → blox → impl → binary), then `generate`.
- **`init <dir>`** — creates a fresh workspace: directory layout (`crates/{messages,context,bloxes,impl}`, `apps`, `spec/...`), a root Cargo.toml with path dependencies on the bloxide checkout this CLI runs from, the blox-spec template, the `building-with-bloxide` skill, an `AGENTS.md`, and a runnable hello-world `counter` app (via the `new-all` code path). An existing non-empty target directory → exit 5 (conflict).

#### `cargo blox add-state` / `remove-state`

```
cargo blox add-state <BLOX_NAME> <STATE_NAME> [--parent <STATE>] [--composite] [--error]
cargo blox remove-state <BLOX_NAME> <STATE_NAME>
```

`add-state` appends a `[[topology.states]]` entry (`composite`, `parent`, and `error` keys are only written when set). A state with the same name already exists → exit 5 (conflict). There is no `--if-not-exists` on this command.

`remove-state` removes the entry. It refuses (exit 1) when other states reference the state as their `parent`. A state that does not exist → exit 3.

**Output (stdout):** `Added state '<state>' to <blox>` / `Removed state '<state>' from <blox>`

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
| `--target` | yes | Target state, or one of the keywords `stay` / `reset` / `stop` / `done` / `fail` |
| `--action` | no (repeatable) | Action function path (e.g. `Self::handle_spawn_worker`), stored verbatim |
| `--guard` | no (repeatable) | Guard condition and target: `"condition:target"` |
| `--feature` | no | Feature gate (e.g. `dynamic`) |
| `--if-not-exists` | no | Exit 0 silently if transition already exists |

**Guard syntax:** `--guard "<condition>:<target>"` where condition is a Rust expression and target is a state name. Multiple guards are added in order. Example:

```
--guard "ctx.spawn_in_flight:Spawning" --guard "ctx.pending == 0:AllDone"
```

**Dedup key:** `state` + `event` pair (exact string comparison). If this pair already exists in the blox's transitions, exit code 5 (conflict) unless `--if-not-exists`, in which case the command exits 0 with no output and the TOML unchanged.

**Output (stdout):** `Added transition <state> + <event> -> <target> to <blox>`

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | Success (or already exists with `--if-not-exists`) |
| 1 | blox.toml unreadable or invalid TOML |
| 2 | Usage error (missing required flag) — handled by clap |
| 5 | Conflict (transition already exists without `--if-not-exists`) |

**TOML output:**

```toml
[[topology.transitions]]
state = "Idle"
event = "PoolMsg::SpawnWorker(_)"
target = "Spawning"
actions = ["Self::handle_spawn_worker"]
feature = "dynamic"
```

With guards:

```toml
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
target = "AllDone"
```

#### `cargo blox remove-transition`

Remove a `[[topology.transitions]]` entry from a blox's `blox.toml`. The nested `[[topology.transitions.guards]]` entries go with it.

```
cargo blox remove-transition <BLOX_NAME> --state <STATE> --event <EVENT>
```

**Arguments:**

| Arg | Required | Description |
|-----|----------|-------------|
| `blox_name` | yes (positional) | Name of the blox crate |
| `--state` | yes | Source state name |
| `--event` | yes | Event pattern |

**Match key:** `state` + `event` pair (exact string comparison). If not found, exit code 3 (not found).

**Output (stdout):** `Removed transition <state> + <event> from <blox>`

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | blox.toml unreadable or invalid TOML |
| 2 | Usage error (missing required flag) — handled by clap |
| 3 | Not found (state+event pair does not exist) |

#### `cargo blox add-entry` / `add-exit` / `remove-entry` / `remove-exit`

Entry and exit hooks live in `[[topology.entry]]` / `[[topology.exit]]` arrays — one hook per state.

```
cargo blox add-entry <BLOX_NAME> --state <STATE> [--action <ACTION>]... [--feature <FEATURE>] [--if-not-exists]
cargo blox add-exit  <BLOX_NAME> --state <STATE> [--action <ACTION>]... [--feature <FEATURE>] [--if-not-exists]
cargo blox remove-entry <BLOX_NAME> --state <STATE>
cargo blox remove-exit  <BLOX_NAME> --state <STATE>
```

**Dedup key:** `state` — a state with an existing hook of that kind conflicts (exit 5) unless `--if-not-exists` (exit 0, silently).

**Output (stdout):** `Added entry hook for state <state> to <blox>` / `Removed exit hook for state <state> from <blox>`

**TOML output:**

```toml
[[topology.entry]]
state = "Active"
actions = ["Self::on_active_entry"]
feature = "dynamic"
```

#### `cargo blox add-message` / `remove-message`

```
cargo blox add-message <CRATE_NAME> <VARIANT_NAME> [name:ty ...]
cargo blox remove-message <CRATE_NAME> <VARIANT_NAME>
```

Fields are trailing positional arguments in `name:ty` form (e.g. `round:u32 payload:Vec<u8>`); a malformed field spec is skipped with a warning on stderr. Fields are stored as nested `[[messages.variants.fields]]` tables.

`add-message` targets the conventional `XxxMsg` `[[messages]]` table (see *Message Table Targeting*) and exits 5 if the variant already exists (no `--if-not-exists` on this command). `remove-message` searches **every** `[[messages]]` table and removes the variant from whichever contains it; no match (or no `[[messages]]` array at all) → exit 3.

**Output (stdout):** `Added variant '<variant>' to <path>` / `Removed variant '<variant>' from <path>`

#### Context commands

`add-use`, `add-field`, and `add-action` append to `[[context.uses]]`, `[[context.fields]]`, and `[[context.actions]]` respectively in the blox's blox.toml, creating the `[context]` section when missing.

- **`add-use`** — two mutually exclusive shapes: single-field (`--field` + `--field-type` + `--role`, given together) or multi-field (`--sub-field name:ty:role`, repeatable; the type may contain `::` paths). `--role` and each sub-field role must be exactly `ctor` or `state` (anything else → exit 1). Dedup key: any field name the entry would contribute — top-level `field` or a sub-field `name` (multi-field entries have no top-level `field`). Single-field writes `field`, `field_type`, `role`, and optional `feature`; multi-field writes optional `feature` plus an inline `fields = [ { name, ty, role }, ... ]` array.
- **`remove-use`** — removes all single-field `[[context.uses]]` entries whose `field` matches `--field`, and removes the matching sub-field from multi-field entries (inline `fields = [...]` or nested `[[context.uses.fields]]`); an entry left with no sub-fields is removed. No match → exit 3.
- **`add-field`** — dedup key: `name`. Writes `name`, `type`, and optional `default`.
- **`remove-field`** — removes the name from `[[context.fields]]`, from single-field `[[context.uses]]` entries, and from multi-field entries (inline `fields = [...]` or nested `[[context.uses.fields]]`) — whichever matches; an entry left with no sub-fields is removed. No match anywhere → exit 3.
- **`add-action`** — Dedup key: `name`. Writes `name`, plus any of `--fn-name`, `--crate-name`, `--module`, `--field` (repeatable, stored as a string array), `--event-payload`, `--impl-required`, `--returns` (only `"ActionResult"` is recognized — anything else → exit 1, mirroring the codegen/lint hard rule), `--feature`.
- **`remove-action`** — removes by `--name`. No match → exit 3.

All three add commands support `--if-not-exists` (exit 0 silently on duplicate).

#### System commands

The system commands edit `apps/<app>/system.toml`. A missing system.toml → exit 3. On success each prints a reminder to run `cargo blox generate` (which regenerates main.rs and Cargo.toml).

- **`add-actor <app> --name <n> --blox <crate>`** — appends an `[[actors]]` entry with `name`, `blox`, optional `impl_crate`, optional `kind`, and `--feature` (repeatable, stored as a string array). Duplicate actor name → exit 5 unless `--if-not-exists`.
- **`remove-actor <app> --name <n>`** — removes the actor and cleans dangling references: the name is dropped from every supervision group's `children` list and from `[supervision.policies]`. Actor not found → exit 3.
- **`add-supervision <app> --supervisor <n> --strategy <s> [--child <actor>]...`** — appends a `[[supervision]]` group with `supervisor`, `strategy`, and `children`. `--strategy` must be exactly `when_any_done` or `when_all_done` (anything else → exit 1). Duplicate supervisor → exit 5 unless `--if-not-exists`.
- **`remove-supervision <app> --supervisor <n>`** — removes the group. Not found → exit 3.
- **`set-policy <app> --actor <n> [--restart-max <n>] [--stop]`** — requires at least one of `--restart-max` / `--stop` (neither → exit 1). With multiple `[[supervision]]` groups it edits the one whose `children` list contains the actor (a single group is edited unconditionally); no group listing the actor → exit 1. Writes an inline table under the group's `policies`:

  ```toml
  [[supervision]]
  supervisor = "bloxide-supervisor"
  strategy = "when_all_done"
  children = ["pool", "worker"]

    [supervision.policies]
    pool = { restart = { max = 3 }, stop = true }
  ```

- **`add-injection <app> --actor <n> --field <f> --from <source>`** — adds an entry to the actor's `[actors.inject]` table. The `--from` grammar:

  | Spec | Written as |
  |------|-----------|
  | `self` | `{ source = "self" }` |
  | `self_secondary[:<index>]` | `{ source = "self_secondary", index = n }` |
  | `actor:<name>[:<field>]` | `{ source = "actor", actor = name, field = f }` |
  | `factory:<crate>:<function>` | `{ source = "factory", crate = c, function = f }` |

  Actor not found → exit 3; malformed `--from` → exit 1.

#### `cargo blox list-bloxes`

List all blox crates in the workspace with summary counts, sorted alphabetically by name.

```
cargo blox list-bloxes [--json]
```

**Default output (table):**

```
NAME        STATES  TRANSITIONS  MESSAGES
counter     1       1            1
ping        4       3            3
pong        1       1            3
```

**JSON output (`--json`):**

```json
[
  {"name": "counter", "states": 1, "transitions": 1, "messages": 1},
  {"name": "ping", "states": 4, "transitions": 3, "messages": 3},
  {"name": "pong", "states": 1, "transitions": 1, "messages": 3}
]
```

`messages` counts the total variants across the `crates/messages/*` crates referenced by the blox's `[[event.mailboxes]]` `message_path` entries (message enums live in dedicated messages crates, not in the blox's own blox.toml); each messages crate is counted at most once. Note: unlike the other commands, the `crates/bloxes/` scan (and the `crates/messages/` lookup) is relative to the current directory — run `list-bloxes` from the workspace root.

#### `cargo blox list-states <blox>`

List all states in a blox's topology.

```
cargo blox list-states <BLOX_NAME> [--json]
```

**Default output (table):**

```
NAME      INITIAL  COMPOSITE  ERROR  PARENT
Idle      true     false      false
Active    false    false      false
```

`PARENT` is empty for root states. Missing blox.toml → exit 3.

**JSON output (`--json`):**

```json
[
  {"name": "Idle", "initial": true, "composite": false, "error": false, "parent": null},
  {"name": "Active", "initial": false, "composite": false, "error": false, "parent": null}
]
```

#### `cargo blox list-messages <crate>`

List all message variants in a messages crate's `blox.toml`, collected across **all** `[[messages]]` entries.

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

Missing blox.toml → exit 3.

#### `cargo blox list-transitions <blox>`

List all transitions in a blox's topology.

```
cargo blox list-transitions <BLOX_NAME> [--json]
```

**Default output (table):**

```
STATE       EVENT                         TARGET      ACTIONS                         GUARDS  FEATURE
Idle        PoolMsg::SpawnWorker(_)       Spawning    Self::handle_spawn_worker       0       dynamic
Spawning    PoolEvent::SpawnReply(_)      Active      Self::handle_spawned_worker     2       dynamic
Spawning    PoolMsg::SpawnWorker(_)       stay        Self::handle_spawn_worker_queued 0      dynamic
Spawning    PoolMsg::WorkDone(_)          stay        Self::handle_work_done          0       —
Active      PoolMsg::SpawnWorker(_)       Spawning    Self::handle_spawn_worker       0       dynamic
Active      PoolMsg::WorkDone(_)          stay        Self::handle_work_done          1       —
```

`GUARDS` shows the guard count; `ACTIONS` and `FEATURE` show `—` when empty/absent. Missing blox.toml → exit 3.

**JSON output (`--json`):**

```json
[
  {
    "state": "Idle",
    "event": "PoolMsg::SpawnWorker(_)",
    "target": "Spawning",
    "actions": ["Self::handle_spawn_worker"],
    "guards": [],
    "feature": "dynamic"
  },
  {
    "state": "Spawning",
    "event": "PoolEvent::SpawnReply(_)",
    "target": "Active",
    "actions": ["Self::handle_spawned_worker"],
    "guards": [
      {"condition": "ctx.spawn_in_flight || !ctx.spawn_queue.is_empty()", "target": "Spawning"},
      {"condition": "ctx.pending == 0 && !ctx.worker_refs.is_empty()", "target": "AllDone"}
    ],
    "feature": "dynamic"
  }
]
```

`feature` serializes as `null` when absent.

#### `cargo blox lint`

Validates every blox.toml in the workspace and reports diagnostics with did-you-mean suggestions.

**Errors** (fail the run, exit 1): transition `state` / `target` / guard `target` / entry / exit / parent referencing undeclared states; duplicate state names and duplicate transitions (same state + event); event patterns referencing unknown variants of *known* enums (the blox's own event enum, workspace message enums, framework enums); `Self::` actions not declared in `[[context.actions]]`; bare action functions missing from `spec_imports`; guard conditions referencing undeclared `ctx.<field>` fields.

**Warnings** (do not fail the run): unreachable states (nothing targets them, not initial, not error); states with no outgoing transitions (events bubble to parents).

#### `cargo blox ci`

Full CI matrix, discovered from the workspace rather than hardcoded: `cargo check --workspace`, per-member feature combos (`alloc` → `--no-default-features`, `std` → `--features std`), the no_std audit matrix from `[workspace.metadata.bloxide-ci]` in the root Cargo.toml, plus tests, fmt, clippy, doc build, and the copyright check.

#### `cargo blox verify`

Round-trip verification: parse and codegen every blox.toml in the workspace, viz-export the whole workspace, JSON round-trip every spec, then compare states, transitions, and context between the declarative config and the exported model. Any mismatch is reported and the command fails (exit 1). `--workspace <dir>` overrides root discovery.

#### `cargo blox wire`

Generates a main.rs from a single system.toml manifest: `--system` (default `<workspace>/system.toml`), `--output` (default `<system dir>/src/main.rs`), `--run` to also run the generated binary crate via `cargo run -p`. Missing system.toml → exit 3. (`cargo blox generate` already regenerates wiring for every system.toml in the workspace; `wire` is the single-manifest form.)

#### `cargo blox viz`

Without flags, launches the Dioxus visualizer from `tools/bloxide-visualizer` (requires a bloxide checkout and the `dx` CLI; the workspace is passed as `BLOXIDE_VIZ_WORKSPACE`). `--port` defaults to 8080; `--open` opens the browser. `--export <dir>` instead writes the blox specs as JSON to that directory and exits without a server. Visualizer directory missing → exit 3.

### Identity Model

| Entity | Natural key | Unique within | Stable? | Self-describing? |
|--------|------------|---------------|---------|-------------------|
| Blox | crate name | workspace | yes | yes |
| State | state name | blox | yes | yes |
| Message | variant name | message enum crate | yes | yes |
| Transition | state + event | blox | yes | yes |
| Entry/exit hook | state | blox (per hook kind) | yes | yes |
| Context use | field | blox | yes | yes |
| Context field | name | blox | yes | yes |
| Context action | name | blox | yes | yes |
| Actor | name | system.toml | yes | yes |
| Supervision group | supervisor | system.toml | yes | yes |
| Injection | actor + field | system.toml | yes | yes |

**No synthetic IDs.** The natural key is the identity. This is consistent with every system studied (Erlang, Akka, XState, Boost.SML, Terraform, Pulumi, Helm, cargo-edit, Rails).

**Why not IDs:**
- State+event is already unique and stable (state names become enum variants; event patterns reference message types)
- IDs add TOML noise the codegen must ignore
- IDs require ID generation, gap handling, and renumbering logic
- IDs are inconsistent with the `add-*`/`remove-*` commands (which use name-based matching)
- `list-transitions --json` gives the agent everything an ID would, without the indirection

### Agent-Friendly CLI Properties

| Property | Status | Notes |
|----------|--------|-------|
| Non-interactive | ✅ | All commands accept args, no prompts |
| `--json` output | ✅ | All `list-*` commands |
| List commands | ✅ | `list-bloxes`, `list-states`, `list-messages`, `list-transitions` |
| Idempotent add | ✅ | Exit code 5 on conflict; `--if-not-exists` on the add commands that accept it |
| Semantic exit codes | ✅ | 0=success, 1=error, 2=usage, 3=not found, 5=conflict |
| Observable state changes | ✅ | Agent runs `list-*` after add/remove to verify |
| Stable identifiers | ✅ | Natural keys are stable and self-describing |
| Clear error messages | ✅ | Errors include the blox name, state, and event that were not found |

### Exit Code Reference

Implemented in `src/exit.rs`. Commands construct `CodedError` values (`not_found` → 3, `conflict` → 5) or return `EditError::{Conflict, NotFound}` from the shared `bloxide_codegen::edit` primitives; `main` maps these to the process exit code (`exit_process`). Any other error exits 1. Clap parse failures exit 2 natively.

| Code | Meaning | When |
|------|---------|------|
| 0 | Success | Any successful operation, or `add-*` with `--if-not-exists` and the entry already exists (silently, no change) |
| 1 | Error (unspecified) | Unreadable/invalid TOML, IO errors, invalid `--strategy` / `--kind` / `--role` / `--from` values, `set-policy` with neither `--restart-max` nor `--stop`, `remove-state` on a state with children, lint or verify failures, cargo subprocess failures |
| 2 | Usage error | Clap-native: missing required argument, unknown flag, unparsable value |
| 3 | Not found | The natural key matches nothing (state, variant, transition pair, hook, context entry, actor, supervision group); also a missing blox.toml (`list-*`, `new-impl`), missing system.toml (system commands, `wire`), or missing visualizer directory (`viz`) |
| 5 | Conflict | `add-*` when the natural key already exists (without `--if-not-exists`); `init` when the target directory exists and is non-empty |

Errors print to stderr as `Error: <message>`; confirmations and all `list-*` output print to **stdout**.

### Workspace Root Resolution

Commands resolve their paths from the **workspace root**, not the current directory: `toml_helpers` walks up from the current directory to the first `Cargo.toml` containing a `[workspace]` section (`find_workspace_root`), falling back to the current directory outside a workspace. All `blox.toml` / `system.toml` paths are built from that root (`crates/bloxes/<name>/blox.toml`, `crates/messages/<name>/blox.toml`, `apps/<name>/system.toml`), so the `add-*` / `remove-*` / `list-states` / `list-transitions` / `list-messages` commands work from any subdirectory.

`generate`, `verify`, and `viz` anchor differently: they start from `CARGO_MANIFEST_DIR` and likewise walk up to the workspace root (overridable with `--workspace` on `generate` / `verify`). `wire`, `watch`, and the `new-*` scaffolding commands resolve the workspace root from the current directory; `wire` defaults `--system` to `<workspace>/system.toml`. `new-all` resolves the root once and threads it through every layer (including its internal `generate` call), so a `new-all` from a subdirectory never mixes roots; `init` creates the target workspace first, then scaffolds inside it. `list-bloxes` is the one exception: its `crates/bloxes/` scan is relative to the current directory.

### TOML Manipulation Convention

All TOML mutation goes through `toml_edit::DocumentMut`, which preserves the original formatting and comments (including the copyright header) — the old `toml::Value` + `toml::to_string` round-trip stripped both and is no longer used anywhere (issue #96).

All `add-*` / `remove-*` commands follow the same internal pattern:

1. **Resolve the manifest path** from the workspace root (see *Workspace Root Resolution*).
2. **Load** the file into a `toml_edit::DocumentMut` (`toml_helpers::load_toml`).
3. **Mutate:**
   - Topology edits (states, transitions, entry/exit hooks) call the shared edit primitives in `bloxide_codegen::edit` — the single write path shared with the visualizer's save functions. These return `EditError::Conflict` / `EditError::NotFound`, which `main` maps to exit codes 5 / 3.
   - Message, context, and system commands (`message_cmd.rs`, `context_cmd.rs`, `system_cmd.rs`) perform their edits inline, also on `DocumentMut` (`message_cmd.rs` was migrated from the comment-stripping `toml::Value` path to the same convention).
4. **Save** with `toml_helpers::save_toml` (`doc.to_string()` — formatting and comments preserved).
5. **Print the confirmation to stdout.**

No codegen is triggered. The user runs `cargo blox generate` separately (the system commands print a reminder).

### Message Table Targeting

`add-message` targets the `[[messages]]` table whose name matches the conventional enum name for the crate: strip a trailing `-messages` suffix from the crate name, CamelCase the remainder, and append `Msg` (e.g. `ping-pong-messages` → `PingPongMsg`). If no such table exists, the **first** `[[messages]]` table is used; if the crate has no `[[messages]]` array at all, one is created (`name = "XxxMsg"`, `visibility = "pub"`). `remove-message` searches every `[[messages]]` table and removes the variant from whichever contains it.

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

## Test Layout

Tests live in the two tool crates.

### `bloxide-codegen`

- **Unit tests** — inline in `src/system_spec.rs` (`#[cfg(test)] mod tests`), covering system.toml parsing and action-config handling.
- **Integration tests:**
  - `tests/codegen.rs` — TOML parsing and `generate_from_toml` / `generate_all` against inline TOML fixtures.
  - `tests/system_wiring.rs` — runs `generate_system_wiring_from_toml` and `generate_cargo_toml` against the real workspace manifests (`apps/*/system.toml`) and asserts structural properties of the generated main.rs / Cargo.toml: channel creation, supervisor setup, injection wiring, bootstrap, runtime selection, dynamic-actor handling, dependency resolution, and feature inference.

### `cargo-blox`

Integration tests only, in `tests/` — one file per command group:

| File | Covers |
|------|--------|
| `tests/add_transition.rs` | Basic add, actions, guards (including `::` in the condition), feature, all options combined, duplicate rejection, `--if-not-exists`, blox not found, missing required arg |
| `tests/remove_transition.rs` | Remove, non-existent pair, remove with nested guards, preserving other transitions, blox not found |
| `tests/list_bloxes.rs` | Table and JSON output, summary counts, empty workspace, missing `crates/bloxes/` dir |
| `tests/list_states.rs` | Table and JSON output, columns, empty blox, blox not found |
| `tests/list_transitions.rs` | Table and JSON output, guards, features, empty blox, blox not found |
| `tests/list_messages.rs` | Table and JSON output, fields, empty crate, crate not found |

**Approach:** each test builds a minimal blox.toml fixture in a `tempfile::TempDir` (under `crates/bloxes/<name>/` or `crates/messages/<name>/`), spawns the compiled binary (`env!("CARGO_BIN_EXE_cargo-blox")`) as a subprocess with the temp dir as its working directory — outside a workspace, path resolution falls back to the current directory — captures stdout/stderr, and asserts on the output and exit status. The add/remove tests additionally read back the mutated blox.toml and assert on its structure (parsed as `toml::Value` in the test, and as raw text where guard removal must be verified). Dev-dependencies: `tempfile`, `toml`.

## Invariants

- No `add-*` / `remove-*` command triggers codegen. The user runs `cargo blox generate` separately (system commands print a reminder).
- Natural keys are the sole identity mechanism. No synthetic IDs are added to `blox.toml` or `system.toml`.
- `list-*` commands are read-only and never modify the TOML.
- All `list-*` commands support `--json` for agent consumption.
- Confirmations and list output go to stdout; errors go to stderr.
- Exit codes are semantic: 0=success, 1=error, 2=usage, 3=not found, 5=conflict.
- Guard parsing splits on the **last** `:` to handle `::` in Rust paths.
- Transition matching uses exact string comparison on `state` and `event` fields.
- All TOML edits go through `toml_edit::DocumentMut` — comments and formatting are preserved. Topology edits share the single write path in `bloxide_codegen::edit`.
- Paths resolve from the workspace root, so commands work from any subdirectory.
- `generate` runs lint first, formats generated files individually with rustfmt, and is idempotent — re-running it with no changes rewrites nothing.
