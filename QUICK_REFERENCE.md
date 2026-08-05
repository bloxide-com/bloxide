# Bloxide Quick Reference

Decision trees and lookup tables for common tasks. Keep this open while you work.

> **NO BACKWARDS COMPATIBILITY.** This project does not preserve backwards
> compatibility — not for APIs, CLIs, config formats, terminology, or
> architecture layers. When something changes, update every call site, every
> doc, and every fixture in the same change, and delete the old form
> completely. Never add legacy aliases, deprecation periods, compatibility
> shims, feature bridges, or "kept for backwards compatibility" notes.

---

## Decision: Where Does New Functionality Go?

```
 ┌─────────────────────────────────────────────────────────────────┐
 │ Does it require async waiting on something OTHER THAN messages? │
 └────────────────────────────┬────────────────────────────────────┘
                    ┌─────────┴─────────┐
                    │                   │
                   YES                  NO
                    │                   │
                    ▼                   ▼
         ┌──────────────────┐    ┌──────────────────────┐
         │ Does it need     │    │ Is it message-driven │
         │ runtime bridges? │    │ only?                │
         └────────┬─────────┘    └──────────┬───────────┘
           ┌──────┴──────┐          ┌────────┴────────┐
           │             │          │                 │
          YES           NO        YES                NO
           │             │          │                 │
           ▼             ▼          ▼                 ▼
    ┌─────────────┐ ┌─────────┐ ┌─────────────┐ ┌─────────────────┐
    │ New stdlib  │ │ Context │ │ Standard    │ │ Context field   │
    │ crate       │ │ field   │ │ run loop    │ │ (sync hardware) │
    │ (timer,     │ │ + direct│ │ (run() +    │ │                 │
    │ supervisor) │ │ access  │ │ RunConfig)  │ │                 │
    └─────────────┘ └─────────┘ └─────────────┘ └─────────────────┘
```

---

## Decision: How Do I Add Mutable State to a Blox?

| Question | Answer | Implementation |
|----------|--------|----------------|
| Is it an ActorRef? | — | `foo_ref: ActorRef<M, R>` in `[[context.uses]]` (auto-detected) |
| Is it the ActorId? | — | `self_id: ActorId` (auto-emitted by codegen) |
| Is it a constructor param (factory)? | Yes | `[[context.uses]]` with `role = "ctor"` |
| Is it state data? | Yes | `[[context.fields]]` entry — direct field, zero-initialized |

State fields are plain fields on the context struct. There is no `B` generic, no behavior object, no accessor traits.

---

## Decision: Do I Need a New Messages Crate?

```
 ┌───────────────────────────────────────────────────────────┐
 │ Is this message type used by 2+ blox crates?            │
 └────────────────────────────┬──────────────────────────────┘
                    ┌─────────┴─────────┐
                    │                   │
                   YES                  NO
                    │                   │
                    ▼                   ▼
         ┌──────────────────┐    ┌───────────────────────┐
         │ Create dedicated │    │ Is it only received   │
         │ *-messages crate │    │ (never sent by other  │
         │ (ping-pong-msgs) │    │ bloxes)?              │
         └──────────────────┘    └────────────┬──────────┘
                                       ┌─────┴─────┐
                                       │           │
                                      YES          NO
                                       │           │
                                       ▼           ▼
                              ┌──────────────┐ ┌───────────────┐
                              │ Define in    │ │ Create shared │
                              │ blox crate   │ │ messages crate│
                              │ (internal)   │ │ anyway        │
                              └──────────────┘ └───────────────┘
```

---

## Decision: Which Runtime Trait Do I Need?

| You want to... | Trait | Layer | Who implements |
|----------------|-------|-------|----------------|
| Create static channels at startup | `StaticChannelCap` | Tier 2 | Runtime (`bloxide-embassy`) |
| Create channels dynamically | `DynamicChannelCap` | Tier 2 | Runtime (Tokio, TestRuntime) |
| Spawn actors dynamically | `SpawnCap` | Tier 2 | Runtime (Tokio, TestRuntime) |
| Get current time, set timers | `TimerService` | Tier 2 | Runtime + `bloxide-timer` |
| Run an actor (any mode) | `run` + `RunConfig` | core fn | `bloxide-core` (re-exported by runtimes) |
| Emergency kill an actor | `KillCapability` | Tier 2 | Runtime (Tokio) |
| Send/receive messages | `BloxRuntime` | Tier 1 | Runtime (blox sees only this) |

---

## Decision: Which State Topology Pattern?

| Pattern | When to Use | Example |
|---------|-------------|---------|
| Flat FSM | Simple linear progression | Counter: Init → Ready → (Decision::Done) |
| Composite + Siblings | Related substates with shared logic | Ping: Operating → (Active, Paused) |
| Hierarchical Cleanup | Parent on_exit cleans up children | Supervisor: Running → [child states] |

---

## Decision: Where Do Tests Go?

| Test Type | Location |
|-----------|----------|
| Blox unit tests (TestRuntime) | `crates/bloxes/*/src/tests.rs` |
| Context crate tests | `crates/context/*/src/tests.rs` |
| Impl crate tests | `crates/impl/*/src/tests.rs` |
| Integration tests (full runtime) | `apps/*-demo/` (system.toml + generated main.rs); app integration tests live in `apps/<app>/tests/` |

---

## Common Patterns Lookup

### Emit a Message

```rust
// In context crate (e.g., blox-ctx-ping-pong):
pub fn send_ping<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    round: u32,
) -> ActionResult {
    ActionResult::from(peer_ref.try_send(self_id, PingPongMsg::Ping(Ping { round })))
}
```

### Timer Pattern

Use `bloxide-timer` and `blox-ctx-ping-pong` action functions instead of manual message construction.

#### Setup

1. Add dependency:
   ```toml
   [dependencies]
   bloxide-timer = { version = "0.0.3", features = ["std"] }
   blox-ctx-ping-pong = { path = "..." }
   ```

2. Add timer fields to context in `blox.toml`:
   ```toml
   [[context.uses]]
   crate = "bloxide_timer"
   field = "timer_ref"
   field_type = "ActorRef<TimerCommand, R>"
   role = "ctor"

   [[context.fields]]
   name = "current_timer"
   type = "Option<TimerId>"
   ```

3. Declare timer action functions in `[[context.actions]]`:
   ```toml
   # schedule_resume(self_id, self_ref, timer_ref, round, current_timer) -> ActionResult
   # (duration computed from the round: 2000 + round × 500 ms)
   [[context.actions]]
   name = "schedule_pause_timer"
   fn_name = "schedule_resume"
   crate = "blox_ctx_ping_pong"
   fields = ["self_id", "self_ref:ref", "timer_ref:ref", "round", "current_timer:mut"]
   impl_required = false

   # cancel_timer_by_id lives in bloxide_timer (module `actions`, also
   # re-exported at the crate root and in the prelude)
   [[context.actions]]
   name = "cancel_pause_timer"
   fn_name = "cancel_timer_by_id"
   crate = "bloxide_timer"
   module = "actions"
   fields = ["self_id", "timer_ref:ref", "current_timer:mut"]
   impl_required = false
   ```

### Spawn a Child Actor

```rust
// In wiring (binary) — factory injection via constructor field:
let pool_ctx = PoolCtx::new(
    pool_id,
    pool_ref,
    spawn_worker_tokio,  // factory closure
);

// In impl crate — free function, no struct, no trait impl.
// SpawnRequest/SpawnedWorker live in `blox_ctx_pool_ref` (pool-messages is plain data);
// SpawnOutput/SpawnFn/SpawnCap live in `bloxide_spawn`.
pub fn spawn_worker<S>(
    req: SpawnRequest<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>,
    notify: ActorRef<ChildLifecycleEvent, TokioRuntime>,
) -> SpawnOutput<TokioRuntime>
where
    S: MachineSpec<Ctx = WorkerCtx<TokioRuntime>>,
    // ... event bounds ...
{
    // ... spawn logic; the generated main.rs monomorphizes S with the
    // system-level concrete spec via a wrapper ...
}
```

---

## Decision: Which Lifecycle Action?

```
 ┌──────────────────────────────────────────────────────────────┐
 │ What lifecycle outcome do you need?                          │
 └────────────────────────────┬─────────────────────────────────┘
                    ┌─────────┴─────────┐
                    │                   │
            Restartable reset        Self-suspend (Stop)
                    │                   │
                    ▼                   ▼
         ┌──────────────────┐    ┌────────────────────┐
         │ LifecycleCommand │    │ Decision::Stop or  │
         │ ::Reset          │    │ LifecycleCommand   │
         │ (via dispatch)   │    │ ::Stop (dispatch)  │
         └──────────────────┘    └────────────────────┘
                    │                   │
                    ▼                   ▼
         ┌──────────────────┐    ┌──────────────────┐
         │ LCA change_state │    │ on_exit chain    │
         │ → enters         │    │ → on_init_entry  │
         │   initial_state  │    │ → task stays     │
         │   immediately    │    │   alive in Init  │
         │   (no Init)      │    │   (suspended)    │
         └──────────────────┘    └──────────────────┘
                    │                   │
                    ▼                   ▼
         ┌──────────────────┐    ┌──────────────────┐
         │ DispatchOutcome  │    │ DispatchOutcome  │
         │ ::Started        │    │ ::Stopped        │
         │ → supervisor     │    │ → supervisor     │
         │   sees Started   │    │   applies        │
         │                  │    │   ChildPolicy    │
         └──────────────────┘    └──────────────────┘
```

### The Five Lifecycle Levels

In increasing severity: **reset → stop → done → abort → kill**.

| Level | Triggered by | Callbacks | Task fate |
|-------|--------------|-----------|-----------|
| Reset | `LifecycleCommand::Reset` / `Decision::Reset` | LCA-based `change_state` to `initial_state()` (skips Init; no `on_init_entry`) | Stays alive, immediately operational |
| Stop | `LifecycleCommand::Stop` / `Decision::Stop` | Full exit chain + `on_init_entry` | Stays alive, suspended in Init (restartable via `Start`) |
| Done | `Decision::Done` | Full exit chain + `on_init_entry` (same ritual as Stop) | Task ends — clean completion; supervisor deregisters (no restart policy) |
| Abort | `AbortCommand` on the abort mailbox (`ChildPolicy::Abort`) | None | Task self-terminates cooperatively |
| Kill | `KillCapability::kill(handle)` (`ChildPolicy::Kill`) | None | Task destroyed externally — permanently dead |

### Failure Is Not a Level — Supervised Tasks Stay Alive

`Decision::Fail` (or entering an `is_error()` state) reports `Failed`. What
happens next depends on `RunConfig.exit_on_fail`:

- **Supervised** (`exit_on_fail = false`): the task **stays alive**, parked in
  its absorbing error state; the supervisor's `ChildPolicy` applies (`Reset`
  revives the actor).
- **Root / unsupervised / bare** (`exit_on_fail = true`): the run loop exits
  and the task ends.

`error_state() -> Option<State>` declares the absorbing error state; the
default (`None`) sends `Decision::Fail` to Init (firing `on_init_entry`)
before reporting `Failed`.

### Emergency Teardown: Abort and Kill (Non-cooperative / Cooperative)

If the actor is non-responsive (stuck in infinite loop, blocking call):
- `ChildPolicy::Abort` sends `AbortCommand` on the child's abort mailbox — the
  task self-terminates cooperatively on receipt. No callbacks fire.
- `ChildPolicy::Kill` calls `KillCapability::kill(handle)` — the external
  ripcord. No callbacks fire; the task is destroyed in place.
- Kill requires a runtime with external task abort (Tokio; Embassy is `NoKill`).
- Kill/Abort policies require abort/kill handles: `ChildGroup::try_add` rejects
  either for a **static** child with `RegistrationError::PolicyRequiresHandles`
  (`Kill` also requires `KillCapability::CAN_KILL` — `try_add_dynamic` returns
  `RegistrationError::KillUnavailable` otherwise) — register via `try_add_dynamic` or
  choose `ChildPolicy::Reset`/`Stop`.
- The supervisor synthesizes `ChildLifecycleEvent::Killed` when it applies
  `ChildPolicy::Kill` (`DispatchOutcome` has no `Killed` variant — the run
  loop never observes one).

### Double Start is Idempotent

If `LifecycleCommand::Start` is dispatched while the machine is already operational:
- Returns `DispatchOutcome::Started(current)` (an ack — mirrors Stop-in-Init)
- Machine stays in current state
- No callbacks fire (no re-entry to `initial_state()`)

This means supervisors can safely send `Start` multiple times without state corruption.

## Declarative Transitions (`blox.toml`)

Transition rules are declared in `blox.toml` under `[[topology.transitions]]`. The codegen (`bloxide-codegen`) emits raw `StateRule { event_tag, matches, actions, guard }` struct literals from these entries — no proc macro is involved.

```toml
# One [[topology.transitions]] entry per transition rule.
# `state`     — which state's handler table owns this rule.
# `event`     — event pattern, e.g. "PingPongMsg::Ping(_)" or "MyMsg::A(_) | MyMsg::B(_)".
# `target`    — fallback target when no guard matches: a state name, "stay",
#               "reset", "stop", "done", or "fail".
# `actions`   — ordered list of action fn paths (called in order, results collected into ActionResults).
# `guards`    — optional list of { condition, target } pairs; evaluated in order; first match wins.
#               `target` is the same vocabulary as the top-level `target` field.
# `feature`   — optional feature gate; the rule is emitted only under #[cfg(feature = "...")].

[[topology.transitions]]
state = "Active"
event = "PingPongMsg::Pong(_)"
target = "Active"
actions = ["Self::forward_ping"]

  [[topology.transitions.guards]]
  condition = "results.any_failed()"
  target = "Error"

  [[topology.transitions.guards]]
  condition = "ctx.round >= MAX_ROUNDS as u32"
  target = "done"

  [[topology.transitions.guards]]
  condition = "ctx.round == PAUSE_AT_ROUND as u32"
  target = "Paused"

  [[topology.transitions.guards]]
  condition = "_"
  target = "stay"

# Multiple patterns for the same state are expressed as separate
# [[topology.transitions]] entries with the same `state`.
```

**Root-level fallback rules** are ordinary `[[topology.transitions]]` entries with the reserved keyword `state = "root"` (`"root"` cannot name a user state). They fire when a domain event bubbles past all user states. `event = "_"` is a catch-all (`WILDCARD_TAG`). `guards` and `feature` gates work as for state-level rules. The codegen emits a `ROOT_RULES` constant plus a `root_transitions()` override in the `MachineSpec` impl.

```toml
[[topology.transitions]]
state = "root"
event = "WorkerMsg::PoisonPill(_)"   # or "_" for a catch-all
target = "reset"
actions = ["Self::log_unhandled"]
```

**Guard expressions** use direct field access (no trait methods, no `B::Type::from()`):
- `ctx.round >= MAX_ROUNDS as u32` — direct field comparison
- `ctx.pending == 0` — direct field comparison
- `results.any_failed()` — method on `ActionResults`
- Boolean operators (`&&`, `||`, `!`) are supported

### Event Pattern Forms

The `event` field is a Rust pattern string. The codegen classifies it by the
first identifier's suffix and emits the appropriate `matches` closure:

| Pattern form | Example | Classification | Generated `matches` closure |
|---|---|---|---|
| Tuple-variant wildcard | `PingPongMsg::Ping(_)` | `*Msg` shorthand | `msg_payload().is_some_and(\|m\| matches!(m, PingPongMsg::Ping(_)))` |
| Struct-variant rest | `MyMsg::Timeout { .. }` | `*Msg` shorthand | `msg_payload().is_some_and(\|m\| matches!(m, MyMsg::Timeout { .. }))` |
| Struct-variant field binding | `MyMsg::Timeout { id }` | `*Msg` shorthand | `msg_payload().is_some_and(\|m\| matches!(m, MyMsg::Timeout { id }))` |
| Full-event (envelope) | `SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. }))` | `FullEvent` | `matches!(ev, SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Stopped { .. })))` |
| Ctrl shorthand | `PeerCtrl::AddPeer(_)` | `*Ctrl` shorthand | `ctrl_payload().is_some_and(\|m\| matches!(m, PeerCtrl::AddPeer(_)))` |
| Or-pattern | `PeerCtrl::AddPeer(_) \| PeerCtrl::RemovePeer(_)` | `*Ctrl` shorthand | single `ctrl_payload()` closure matching both arms |
| Wildcard | `_` | `FullEvent` | `matches!(ev, _)` (always true) |

**`{ field_name }` binding syntax.** For struct-variant enums, the pattern
`MyMsg::Timeout { id }` binds the `id` field. The codegen passes the pattern
verbatim to `matches!`, so any valid Rust struct pattern is accepted —
including `{ .. }` (rest, no binding), `{ id }` (bind one field), and
`{ id, .. }` (bind one, ignore the rest).

> **Binding scope — important.** A `{ field_name }` binding is scoped to the
> `matches!` macro inside the `matches` closure. It is **not** visible in the
> `guard` closure or in `actions`, which are separate function pointers with
> their own parameter lists (`|ctx, results, _ev|` for guards,
> `fn(&mut Ctx, &Event)` for actions). The `matches` closure returns `bool`;
> the bound name cannot escape it.
>
> To use a field's value in a guard or action:
> 1. Use `{ .. }` (no binding) in the `event` pattern — it matches the
>    variant without creating an unused binding.
> 2. Write an action that destructures `&Event` and stores the value in
>    `&mut Ctx` (e.g. `ctx.last_timer_id = id`).
> 3. Reference `ctx.*` in the guard condition.
>
> If you do write `{ id }` in the pattern, the bound `id` is unused and will
> trigger an `unused variable` warning unless suppressed; prefer `{ .. }`.

**Event pattern classification** (handled by the codegen, not the user):
- `Enum::Variant(...)` → full-event match closure
- `*Msg` suffix (e.g. `PingPongMsg::Ping(_)`) → `msg_payload()` closure
- `*Ctrl` suffix (e.g. `PeerCtrl::AddPeer(_)`) → `ctrl_payload()` closure

**Target vocabulary**: `"StateName"` → `Decision::Transition(LeafState::new(...))`; `"stay"` → `Decision::Stay`; `"reset"` → `Decision::Reset`; `"stop"` → `Decision::Stop`; `"done"` → `Decision::Done`; `"fail"` → `Decision::Fail`. (The TOML keys `guards`/`condition` keep their names — only the Rust enum is `Decision`.)

---

## File Location Quick Reference

| File Type | Location Pattern |
|-----------|------------------|
| Blox crate | `crates/bloxes/<name>/` |
| Messages crate | `crates/messages/<name>-messages/` |
| Context crate | `crates/context/<name>/` or `crates/bloxide-<service>/` |
| Impl crate (optional) | `crates/impl/<name>-impl/` |
| Binary | `apps/<name>-demo/` (system.toml + generated main.rs) |
| Blox spec | `spec/bloxes/<name>.md` |

---

## Common Error Messages

| Error | Meaning | Fix |
|-------|---------|-----|
| "state X is not a leaf" | Transition target has children | Use leaf state as target |
| "no matching rule" | Event bubbled to root and no handler | Add rule to appropriate state |
| "cannot borrow as mutable" | Guard borrows `&ctx` after actions | Separate action logic from guard logic |
| "trait bound not satisfied" | Runtime missing capability | Add feature flag or use different runtime |

---

## Key Invariants Checklist

The canonical invariant list lives in `AGENTS.md` → "Key Invariants" — read
that, not a copy. Quick sanity checks for the most commonly violated ones:

- [x] Blox crates are generic over `R: BloxRuntime`, with no runtime or `bloxide-log` dependency
- [x] Messages contain only plain data (no `ActorRef`)
- [x] Transition targets are leaf states only; guards use direct field access
- [x] No `actions.rs`, no `crates/actions/` directory, no `B` generic, no accessor traits

---

## cargo blox Command Reference

The canonical list of `cargo blox` subcommands (from `crates/tools/cargo-blox/src/main.rs`;
see `spec/architecture/17-cli-design.md` for the full design). All commands resolve the
workspace root, so they work from any subdirectory. Semantic exit codes: `0` success,
`1` other error, `2` usage error (clap), `3` not found, `5` conflict (already exists).
`generate` runs the spec-to-code lint first and is idempotent. When working from a source
checkout of this repo, run the CLI as `cargo run -p cargo-blox -- blox ...` — the
`cargo blox` on PATH is an installed binary and may be stale.

| Command | Purpose |
|---|---|
| `generate [--workspace <path>]` | Lint, then generate code from all blox.toml + system.toml files in the workspace |
| `build` / `check` / `test` / `run` | `generate`, then the corresponding cargo command |
| `watch` | Watch and regenerate on changes |
| `wire --system <path>` | Generate a binary `main.rs` from a system.toml wiring manifest (`--run` to execute after) |
| `verify` | Round-trip check: blox.toml → codegen → viz-export → JSON → compare |
| `lint` | Spec-to-code lint checks |
| `ci` | Full CI feature matrix |
| `init <dir> [--runtime tokio\|embassy]` | Bootstrap a new bloxide workspace |
| `viz [--export <dir>] [--port N] [--open]` | Launch the visualizer (or export specs as JSON) |
| `new <name> [--messages M] [--context C]` | Scaffold a new blox crate (+ `spec/bloxes/<name>.md`) |
| `new-messages <name>` | Scaffold a new messages crate |
| `new-context <name>` | Scaffold a new context (action-functions) crate |
| `new-impl <name> --blox <blox>` | Scaffold a new impl crate for a blox |
| `new-binary <name> [--runtime tokio\|embassy]` | Scaffold a new wiring binary crate |
| `new-all <name> [--runtime ...]` | Scaffold all layers (messages, context, blox, impl, binary) |
| `list-bloxes [--json]` | List all blox crates in the workspace |
| `list-states <blox> [--json]` | List states in a blox |
| `list-transitions <blox> [--json]` | List transitions in a blox |
| `list-messages <crate> [--json]` | List message variants in a messages crate |
| `add-state <blox> <state> [--parent P] [--composite] [--error]` | Add a state to a topology |
| `remove-state <blox> <state>` | Remove a state |
| `add-transition <blox> --state S --event E --target T [--action ...] [--guard ...]` | Add a transition |
| `remove-transition <blox> --state S --event E [--feature F]` | Remove a transition (`--feature` targets a gated variant) |
| `add-entry <blox> --state S [--action ...]` | Add an entry hook to a state |
| `remove-entry <blox> --state S` | Remove an entry hook |
| `add-exit <blox> --state S [--action ...]` | Add an exit hook to a state |
| `remove-exit <blox> --state S` | Remove an exit hook |
| `add-message <crate> <Variant> [field:ty ...]` | Add a message variant |
| `remove-message <crate> <Variant>` | Remove a message variant |
| `add-use <blox> --field F --field-type T --role ctor\|state` | Add a single-field `[[context.uses]]` entry |
| `add-use <blox> --sub-field N:T:ctor\|state ...` | Add a multi-field `[[context.uses]]` entry |
| `remove-use <blox> --field F` | Remove a use entry (or a sub-field from a multi-field entry) |
| `add-field <blox> --name N --ty T [--default D]` | Add a `[[context.fields]]` state field |
| `remove-field <blox> --name N` | Remove a context field (fields, uses, or uses sub-fields) |
| `add-action <blox> --name N [--field ...] [--crate-name C] [--fn-name F] [--returns ActionResult] ...` | Add a `[[context.actions]]` entry |
| `remove-action <blox> --name N` | Remove a `[[context.actions]]` entry |
| `add-actor <app> --name N --blox B [--impl-crate C] [--kind dynamic\|timer] [--feature F ...]` | Add an actor to a system.toml (no `kind` = static) |
| `remove-actor <app> --name N` | Remove an actor (also cleans supervision refs) |
| `add-supervision <app> --supervisor S --strategy when_any_done\|when_all_done [--child C ...]` | Add a supervision section to a system.toml |
| `remove-supervision <app> --supervisor S` | Remove a supervision section |
| `set-policy <app> --actor A [--stop \| --restart-max N]` | Set a child policy in a supervision section |
| `add-injection <app> --actor A --field F --from X` | Add a constructor injection to an actor |

All `add-*` commands accept `--if-not-exists` (tolerate conflicts, exit 0). All `list-*`
commands accept `--json`. Strategy vocabulary is exactly `when_any_done` / `when_all_done`
(maps to `GroupShutdown` variants — unknown values are hard errors).

---

## See Also

- **Full blox-building workflow**: `skills/building-with-bloxide/SKILL.md`
- **Macro syntax reference**: `skills/building-with-bloxide/reference.md`
- **Key invariants (canonical)**: `AGENTS.md` → "Key Invariants"
- **Architecture overview**: `spec/architecture/00-layered-architecture.md`
