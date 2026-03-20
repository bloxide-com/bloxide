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
| Is it an ActorRef? | — | `#[provides(HasXRef<R>)]` |
| Is it the ActorId? | — | `#[self_id]` |

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
| Emergency kill an actor | `KillCap` | Tier 2 | Runtime (Tokio only) |
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
| Integration tests (full runtime) | `examples/*-demo.rs` or `tests/` |

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
       #[self_id]
       pub self_ref: ActorRef<MyMsg, R>,
       #[provides(TimerRef)]
       pub timer_ref: ActorRef<TimerCommand, R>,  // Required
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

```rust
transitions! {
    // Match the timer message by ID
    MyMsg::Timeout { id } if *id == expected_id => {
        actions: handle_timeout,
        guard: |ctx, results, event| Guard::Transition(LeafState::new(State::TimedOut)),
    }
}
```

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

If the actor is non-responsive (stuck in infinite loop, blocking call), use `KillCap::kill(actor_id)`:
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

| Annotation | Generates | Use When |
|------------|-----------|----------|
| `#[self_id]` | `fn self_id(&self) -> ActorId` | Always (required) |
| `#[provides(HasXRef<R>)]` | `fn x_ref(&self) -> &ActorRef<Msg, R>` | Field is an ActorRef |
| `#[delegates(Trait)]` | Delegates trait impl to field | Binary injects behavior |
| `#[ctor]` | Marks for constructor injection | Custom initialization |

### `transitions!` Syntax

```rust
transitions![
    EventPattern => {
        actions [action_fn1, action_fn2]  // Called in order, results in ActionResults
        guard(ctx, results) {
            condition1 => NextState1,
            condition2 => NextState2,
            _ => stay,  // or `Transition(CurrentState::Leaf)`
        }
    },
    // Multiple patterns for same state:
    EventPattern1 | EventPattern2 => {
        actions [...]
        guard(ctx, results) { ... }
    },
],
```

---

## File Location Quick Reference

| File Type | Location Pattern |
|-----------|------------------|
| Blox crate | `crates/bloxes/<name>/` |
| Messages crate | `crates/messages/<name>-messages/` |
| Actions crate | `crates/actions/<name>-actions/` |
| Impl crate | `crates/impl/<name>-impl/` |
| Binary | `examples/<name>-demo.rs` |
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
