# Bloxide Quick Reference

Decision trees and lookup tables for common tasks. Keep this open while you work.

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
    │ (timer,     │ │ + direct│ │ (run_root)  │ │                 │
    │ supervisor) │ │ access  │ │             │ │                 │
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

State fields are plain fields on the context struct. There is no `B` generic, no behavior object.

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
| Create static channels at startup | `StaticChannelCap` | Tier 2 | Runtime (`bloxide-embassy`, `bloxide-tokio`) |
| Create channels dynamically | `DynamicChannelCap` | Tier 2 | Runtime (Tokio only) |
| Spawn actors dynamically | `SpawnCap` | Tier 2 | Runtime (Tokio only) |
| Get current time, set timers | `TimerService` | Tier 2 | Runtime + `bloxide-timer` |
| Run a supervised actor | `SupervisedRunLoop` | Tier 2 | Runtime |
| Emergency kill an actor | `KillCapability` | Tier 2 | Runtime (Tokio) |
| Send/receive messages | `BloxRuntime` | Tier 1 | Runtime (blox sees only this) |

---

## Decision: Which State Topology Pattern?

| Pattern | When to Use | Example |
|---------|-------------|---------|
| Flat FSM | Simple linear progression | Counter: Init → Ready → (Guard::Stop) |
| Composite + Siblings | Related substates with shared logic | Ping: Operating → (Active, Paused) |
| Hierarchical Cleanup | Parent on_exit cleans up children | Supervisor: Running → [child states] |

---

## Decision: Where Do Tests Go?

| Test Type | Location |
|-----------|----------|
| Blox unit tests (TestRuntime) | `crates/bloxes/*/src/tests.rs` |
| Context crate tests | `crates/context/*/src/tests.rs` |
| Impl crate tests | `crates/impl/*/src/tests.rs` |
| Integration tests (full runtime) | `apps/*-demo/` (system.toml + generated main.rs) or `tests/` |

---

## Common Patterns Lookup

### Emit a Message

```rust
// In context crate (e.g., bloxide-messaging):
pub fn send_ping<R: BloxRuntime>(
    self_id: ActorId,
    peer_ref: &ActorRef<PingPongMsg, R>,
    round: u32,
) {
    let _ = peer_ref.try_send(self_id, PingPongMsg::Ping(Ping { round }));
}
```

### Timer Pattern

Use `bloxide-timer` and `blox-ctx-current-timer` action functions instead of manual message construction.

#### Setup

1. Add dependency:
   ```toml
   [dependencies]
   bloxide-timer = { version = "0.1", features = ["std"] }
   blox-ctx-current-timer = { path = "..." }
   ```

2. Add timer fields to context in `blox.toml`:
   ```toml
   [[context.uses]]
   crate = "bloxide_timer"
   trait = "HasTimerRef<R>"
   field = "timer_ref"
   field_type = "ActorRef<TimerCommand, R>"
   role = "accessor"

   [[context.fields]]
   name = "current_timer"
   type = "Option<TimerId>"
   ```

3. Declare timer action functions in `[[context.actions]]`:
   ```toml
   [[context.actions]]
   name = "schedule_pause_timer"
   fn_name = "schedule_resume"
   crate = "blox_ctx_current_timer"
   kind = "transition"
   fields = ["self_id", "self_ref:ref", "timer_ref:ref", "round", "current_timer:mut"]
   impl_required = false

   [[context.actions]]
   name = "cancel_pause_timer"
   fn_name = "cancel_timer_by_id"
   crate = "blox_ctx_current_timer"
   kind = "transition"
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

// In impl crate — free function, no struct, no trait impl:
pub fn spawn_worker(
    req: SpawnRequest<PeerCtrl<WorkerMsg, TokioRuntime>, TokioRuntime>,
    notify: ActorRef<ChildLifecycleEvent, TokioRuntime>,
) -> SpawnOutput<TokioRuntime> {
    // ... spawn logic ...
}
```

---

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
         ┌──────────────────┐    ┌──────────────────┐
         │ LifecycleCommand │    │ Guard::Stop or   │
         │ ::Reset          │    │ LifecycleCommand │
         │ (via dispatch)   │    │ ::Stop (dispatch)│
         └──────────────────┘    └──────────────────┘
                    │                   │
                    ▼                   ▼
         ┌──────────────────┐    ┌──────────────────┐
         │ on_exit chain    │    │ on_exit chain    │
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

### Emergency Kill (Non-cooperative)

If the actor is non-responsive (stuck in infinite loop, blocking call), use `KillCapability::kill(handle)`:
- **No callbacks fire** — immediate task abort
- Only available in Tokio (Embassy lacks abort support)
- Supervisor tracks killed children separately (no `ChildLifecycleEvent`)

### Double Start is Idempotent

If `LifecycleCommand::Start` is dispatched while the machine is already operational:
- Returns `DispatchOutcome::HandledNoTransition`
- Machine stays in current state
- No callbacks fire (no re-entry to `initial_state()`)

This means supervisors can safely send `Start` multiple times without state corruption.

## Declarative Transitions (`blox.toml`)

Transition rules are declared in `blox.toml` under `[[topology.transitions]]`. The codegen (`bloxide-codegen`) emits raw `StateRule { event_tag, matches, actions, guard }` struct literals from these entries — no proc macro is involved.

```toml
# One [[topology.transitions]] entry per transition rule.
# `state`     — which state's handler table owns this rule.
# `event`     — event pattern, e.g. "PingPongMsg::Ping(_)" or "MyMsg::A(_) | MyMsg::B(_)".
# `target`    — fallback target when no guard matches: a state name, "stay", "reset", or "stop".
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
  target = "stop"

  [[topology.transitions.guards]]
  condition = "ctx.round == PAUSE_AT_ROUND as u32"
  target = "Paused"

  [[topology.transitions.guards]]
  target = "stay"

# Multiple patterns for the same state are expressed as separate
# [[topology.transitions]] entries with the same `state`.
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

**Target vocabulary**: `"StateName"` → `Guard::Transition(LeafState::new(...))`; `"stay"` → `Guard::Stay`; `"reset"` → `Guard::Reset`; `"stop"` → `Guard::Stop`; `"fail"` → `Guard::Fail`.

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

- [ ] `bloxide-core` imports only `futures-core` (no Tokio/Embassy)
- [ ] Blox crates are generic over `R: BloxRuntime`
- [ ] Messages contain only plain data (no `ActorRef`)
- [ ] Transition targets are leaf states only
- [ ] `on_entry` / `on_exit` are infallible (`fn(&mut Ctx)`)
- [ ] Actions called before guard (side effects in actions, pure checks in guard)
- [ ] No catch-all rule that manually returns parent — bubbling is automatic
- [ ] `is_error` states report `Failed`; actors self-stop via `Guard::Stop` (no `is_terminal`)
- [ ] No `B` generic on any `Ctx` or `Spec` type
- [ ] No `#[delegatable]` attribute anywhere
- [ ] No `#[delegates]` annotation anywhere
- [ ] No `actions.rs` file in any blox crate
- [ ] No `crates/actions/` directory
- [ ] No `bloxide-log` dependency in any blox crate
- [ ] No `behavior: B` field in any context struct
- [ ] Guard expressions use direct field access (no trait methods, no `B::Type::from()`)

---

## See Also

- **Full blox-building workflow**: `skills/building-with-bloxide/SKILL.md`
- **Macro syntax reference**: `skills/building-with-bloxide/reference.md`
- **Key invariants (canonical)**: `AGENTS.md` → "Key Invariants"
- **Architecture overview**: `spec/architecture/00-layered-architecture.md`
