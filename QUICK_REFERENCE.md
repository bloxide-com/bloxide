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
| Does the binary need to inject it? | Yes | Behavior trait + `#[delegates]` on field |
| Is it specific to this blox only? | Yes | Direct field, no annotation (Default::default()) |
| Is it an ActorRef? | — | `foo_ref: ActorRef<M, R>` (auto-detected) |
| Is it the ActorId? | — | `self_id: ActorId` (auto-detected) |

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
         ┌──────────────────┐    ┌───────────────────────────────┐
         │ Create dedicated │    │ Is it only received (never   │
         │ *-messages crate │    │ sent by other bloxes)?        │
         │ (ping-pong-msgs) │    └────────────┬──────────────────┘
         └──────────────────┘          ┌─────┴─────┐
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
| Flat FSM | Simple linear progression | Counter: Init → Ready → Done |
| Composite + Siblings | Related substates with shared logic | Ping: Operating → (Active, Paused) |
| Hierarchical Cleanup | Parent on_exit cleans up children | Supervisor: Running → [child states] |

---

## Decision: Where Do Tests Go?

| Test Type | Location |
|-----------|----------|
| Blox unit tests (TestRuntime) | `crates/bloxes/*/src/tests.rs` |
| Action crate tests | `crates/actions/*/src/tests.rs` |
| Integration tests (full runtime) | `apps/*-demo/` (system.toml + generated main.rs) or `tests/` |

---

## Common Patterns Lookup

### Emit a Message

```rust
// In action crate:
pub fn send_foo<R: BloxRuntime>(ctx: &mut impl HasFooRef<R>) {
    ctx.foo_ref().send(FooMsg::Bar(Bar { value: 42 })).unwrap_or_else(|e| {
        blox_log_error!("failed to send Bar: {:?}", e);
    });
}
```

## Timer Pattern

Use `bloxide-timer` action functions instead of manual message construction.

### Setup

1. Add dependency:
   ```toml
   [dependencies]
   bloxide-timer = { version = "0.1", features = ["std"] }
   ```

2. Add timer fields to context:
   ```rust
   #[derive(BloxCtx)]
   pub struct MyCtx<R: BloxRuntime> {
       pub self_id: ActorId,
       pub self_ref: ActorRef<MyMsg, R>,
       pub timer_ref: ActorRef<TimerCommand, R>,  // Auto-detected (matches HasTimerRef::timer_ref)
   }
   ```

3. Implement `HasTimerRef` (auto-derived via `#[provides(TimerRef)]`).

### Setting a Timer

```rust
use bloxide_timer::{set_timer, next_timer_id, TimerCommand};

// In an action function:
fn start_timeout<R: BloxRuntime>(ctx: &mut MyCtx<R>, event: &MyEvent) {
    let timer_id = next_timer_id();
    set_timer(ctx, timer_id, Duration::from_secs(5), MyMsg::Timeout { id: timer_id });
}
```

### Canceling a Timer

```rust
use bloxide_timer::cancel_timer;

fn cancel_timeout<R: BloxRuntime>(ctx: &mut MyCtx<R>, timer_id: TimerId) {
    cancel_timer(ctx, timer_id);
}
```

### Handling Timer Expiration

```toml
# In blox.toml — declare a transition that matches the timer message.
# The action extracts the timer id from the event and stores it in context;
# the guard then inspects context (not the event) to decide the next state.
[[topology.transitions]]
state = "Active"
event = "MyMsg::Timeout { .. }"
actions = ["record_timer_id"]
guards = [
  { condition = "ctx.last_timer_id() == ctx.expected_id", target = "TimedOut" },
  { condition = "_", target = "stay" },
]
```

> **Why the guard reads context, not a bound variable:** the codegen emits
> the `matches` predicate and the `guard` closure as **separate** function
> pointers (`fn(&Event) -> bool` and `fn(&Ctx, &ActionResults, &Event) -> Guard`).
> A binding like `{ id }` is scoped to the `matches!` macro inside the
> `matches` closure and is **not** visible in the guard or in actions.
> To act on a field's value, have an action extract it from `&Event` and
> store it in `&mut Ctx`; the guard then reads it from `&Ctx`.
> The `{ .. }` struct-rest pattern (shown above) is the idiomatic way to
> match a struct variant without binding — see "Event Pattern Forms" below.

### Spawn a Child Actor

```rust
// In wiring (binary):
let spawn_fn = pool_ctx.worker_factory.clone();
let (worker_ref, worker_ctrl) = spawn_fn(spawner, pool_ref).await;

// In blox crate (via action crate):
pub fn spawn_worker<R: BloxRuntime + SpawnCap>(ctx: &mut impl HasWorkerFactory<R>) {
    let factory = ctx.worker_factory();
    // ... spawn logic
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
            Restartable reset        Permanent stop
                    │                   │
                    ▼                   ▼
         ┌──────────────────┐    ┌──────────────────┐
         │ LifecycleCommand │    │ LifecycleCommand │
         │ ::Reset      │    │ ::Stop           │
         │ (via dispatch)   │    │ (via dispatch)   │
         └──────────────────┘    └──────────────────┘
                    │                   │
                    ▼                   ▼
         ┌──────────────────┐    ┌──────────────────┐
         │ on_exit chain    │    │ on_exit chain    │
         │ → on_init_entry  │    │ → on_init_entry  │
         │ → task stays     │    │ → task exits     │
         │   alive in Init  │    │   permanently    │
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

## Macro Quick Reference

### `#[derive(BloxCtx)]` Annotations

Context fields are defined via `[[context.uses]]` in `blox.toml`. The codegen auto-emits `self_id` (first field) and `behavior` (last field, when delegatable uses exist).

| Field | Source | Generates |
|-------|--------|-----------|
| `self_id: ActorId` | Auto-emitted (always) | `fn self_id(&self) -> ActorId` |
| `foo_ref: ActorRef<M, R>` | `[[context.uses]]` with `field = "foo_ref"` | `fn foo_ref(&self) -> &ActorRef<Msg, R>` |
| `foo_factory: fn(...) -> ...` | `[[context.uses]]` with `role = "ctor"` | `fn foo_factory(&self) -> ...` |
| `behavior: B` | Auto-emitted (when delegatable uses exist) | `#[delegates(Trait1, Trait2)]` forwarding impls |

### Declarative Transitions (`blox.toml`)

Transition rules are declared in `blox.toml` under `[[topology.transitions]]`. The codegen (`bloxide-codegen`) emits raw `StateRule { event_tag, matches, actions, guard }` struct literals from these entries — no proc macro is involved.

```toml
# One [[topology.transitions]] entry per transition rule.
# `state`     — which state's handler table owns this rule.
# `event`     — event pattern, e.g. "PingPongMsg::Ping(_)" or "MyMsg::A(_) | MyMsg::B(_)".
# `target`    — fallback target when no guard matches: a state name, "stay", "reset", or "fail".
# `actions`   — ordered list of action fn paths (called in order, results collected into ActionResults).
# `guards`    — optional list of { condition, target } pairs; evaluated in order; first match wins.
#               `target` is the same vocabulary as the top-level `target` field.
# `feature`   — optional feature gate; the rule is emitted only under #[cfg(feature = "...")].

[[topology.transitions]]
state = "Active"
event = "PingPongMsg::Pong(_)"
actions = ["log_pong_received", "forward_ping"]
guards = [
  { condition = "results.any_failed()", target = "Error" },
  { condition = "ctx.round() >= MAX_ROUNDS", target = "Done" },
  { condition = "ctx.round() == PAUSE_AT_ROUND", target = "Paused" },
  { condition = "_", target = "Active" },  # default / self-transition
]

# Multiple patterns for the same state are expressed as separate
# [[topology.transitions]] entries with the same `state`.
```

### Event Pattern Forms

The `event` field is a Rust pattern string. The codegen classifies it by the
first identifier's suffix and emits the appropriate `matches` closure:

| Pattern form | Example | Classification | Generated `matches` closure |
|---|---|---|---|
| Tuple-variant wildcard | `PingPongMsg::Ping(_)` | `*Msg` shorthand | `msg_payload().is_some_and(\|m\| matches!(m, PingPongMsg::Ping(_)))` |
| Struct-variant rest | `MyMsg::Timeout { .. }` | `*Msg` shorthand | `msg_payload().is_some_and(\|m\| matches!(m, MyMsg::Timeout { .. }))` |
| Struct-variant field binding | `MyMsg::Timeout { id }` | `*Msg` shorthand | `msg_payload().is_some_and(\|m\| matches!(m, MyMsg::Timeout { id }))` |
| Full-event (envelope) | `SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Done { .. }))` | `FullEvent` | `matches!(ev, SupervisorEvent::Child(Envelope(_, ChildLifecycleEvent::Done { .. })))` |
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
>    `&mut Ctx` (e.g. `ctx.set_last_timer_id(id)`).
> 3. Reference `ctx.*` in the guard condition.
>
> If you do write `{ id }` in the pattern, the bound `id` is unused and will
> trigger an `unused variable` warning unless suppressed; prefer `{ .. }`.

**Event pattern classification** (handled by the codegen, not the user):
- `Enum::Variant(...)` → full-event match closure
- `*Msg` suffix (e.g. `PingPongMsg::Ping(_)`) → `msg_payload()` closure
- `*Ctrl` suffix (e.g. `PeerCtrl::AddPeer(_)`) → `ctrl_payload()` closure

**Target vocabulary**: `"StateName"` → `Guard::Transition(LeafState::new(...))`; `"stay"` → `Guard::Stay`; `"reset"` → `Guard::Reset`; `"fail"` → `Guard::Fail`.

---

## File Location Quick Reference

| File Type | Location Pattern |
|-----------|------------------|
| Blox crate | `crates/bloxes/<name>/` |
| Messages crate | `crates/messages/<name>-messages/` |
| Actions crate | `crates/actions/<name>-actions/` |
| Impl crate | `crates/impl/<name>-impl/` |
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
- [ ] `is_error` takes precedence over `is_terminal`

---

## See Also

- **Full blox-building workflow**: `skills/building-with-bloxide/SKILL.md`
- **Macro syntax reference**: `skills/building-with-bloxide/reference.md`
- **Key invariants (canonical)**: `AGENTS.md` → "Key Invariants"
- **Architecture overview**: `spec/architecture/00-layered-architecture.md`
