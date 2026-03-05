# Handler and Topology Patterns

This document defines named, reusable patterns for event handler rules and blox state topologies. When building a blox, refer to these patterns by name in your spec and implementation. AI agents should use these patterns as the canonical vocabulary for describing blox behavior.

---

## Handler Model

Each state's `StateFns` contains a `transitions` slice of `TransitionRule` structs. The engine evaluates rules in declaration order; the first matching rule wins. If no rule matches, the event implicitly bubbles to the parent state.

```rust
pub struct TransitionRule<S: MachineSpec, G> {
    pub event_tag: u8,                                              // fast pre-filter
    pub matches: fn(&S::Event) -> bool,                             // does this rule apply?
    pub actions: &'static [fn(&mut S::Ctx, &S::Event) -> ActionResult], // side effects (mutable context)
    pub guard: fn(&S::Ctx, &ActionResults, &S::Event) -> G,        // transition decision (read-only)
}
```

The ordering enforces the invariant: **actions always precede the guard**. The borrow checker enforces that **guards cannot mutate context** (`&Ctx`, not `&mut Ctx`).

Root-level rules use `StateRule<S>`, the same type as state-level rules — both use `Guard<S>`. `Guard::Reset` is available at all levels — state rules and root rules have identical guard capabilities.

---

## Per-Rule Handler Patterns

These patterns describe a single `TransitionRule` within a state's `transitions` slice.

### 1. Pure Transition

No side effects. The event itself is the complete signal.

```rust
transitions![
    MyEvent::Foo(_) => { transition MyState::Bar }
]
```

Use when: the event variant alone determines the next state, with no context inspection needed.

**Example**: `PingEvent::Msg(PingPongMsg::Resume)` always transitions to `Active`.

---

### 2. Sink (Absorb)

Match and absorb the event. Prevents bubbling to the parent state.

```rust
transitions![
    MyEvent::Foo(_) => stay,
]
```

Use when: a parent composite state should silence an event that a child state doesn't handle.

**Example**: `Operating` absorbs stray `PingPongMsg::Pong` events while `Paused` is active.

---

### 3. Action-Then-Stay

Side effects with no state change. The event is handled locally.

```rust
transitions![
    PingPongMsg::Ping(ping) => {
        actions [Self::reply_pong_action]
        stay
    },
]
```

Use when: an event triggers side effects but no state change (e.g., fire-and-forget response).

**Example**: `Pong`'s `Ready` state receives `PingPongMsg::Ping`, sends back a Pong reply, stays in `Ready`.

---

### 4. Action-Then-Guard

Side effects followed by a conditional transition. The most common pattern.

```rust
transitions![
    PingPongMsg::Pong(pong) => {
        actions [Self::log_pong_received, Self::forward_ping]
        guard(ctx, results) {
            results.any_failed()                          => PingState::Error,
            ctx.round() >= B::Round::from(MAX_ROUNDS)     => PingState::Done,
            ctx.round() == B::Round::from(PAUSE_AT_ROUND) => PingState::Paused,
            _                                             => PingState::Active,
        }
    },
]
```

Use when: context is updated or messages are sent, and the resulting context (or action results) determines the next state.

**Example**: `Ping`'s `Active` state logs the round and sends the next ping (`forward_ping`), then guards on `results.any_failed()` first (error priority), then round counters to decide between `Error`, `Done`, `Paused`, or self-transition back to `Active`.

---

### 5. Pure Guard

No side effects. The guard reads context to decide the transition.

```rust
transitions![
    MyEvent::Tick(_) => {
        guard(ctx, _results) {
            ctx.deadline_elapsed => MyState::Timeout,
            _                    => stay,
        }
    },
]
```

Use when: context state (not the event) determines the transition, and the event is merely a trigger.

---

### 6. Bubble (Implicit)

No rule is needed. When no rule matches, the engine automatically bubbles the event to the parent state.

The "Bubble" pattern is the **absence of a rule**. To make bubbling explicit in code, simply do not add a rule for the event variant.

Use when: a leaf state does not handle an event and wants its parent (or root) to handle it.

**Example**: `Done` state has an empty `transitions: &[]` — all events bubble to root, which silently drops them (or handles any root rules you define).

---

## Root Rule Patterns

Root rules use `StateRule<S>` with `Guard` (`Transition`, `Stay`, or `Reset`). Root rules are the same type as state-level rules — `root_transitions()` returns `&'static [StateRule<Self>]`.

**In the new runtime model, supervised actors do not need lifecycle root rules.** The runtime handles Start and Terminate commands via `machine.start()` and `machine.reset()` through a runtime-internal channel — actors never see lifecycle commands as domain events.

`root_transitions()` has a default empty implementation (`&[]`) and is **optional** for most actors. Override it only if you need fallback rules that apply when an event bubbles past all user-declared states:

```rust
// Default — no override needed for most actors:
fn root_transitions() -> &'static [StateRule<Self>] { &[] }
```

### `reset` in State-Level Transitions

Since `Guard::Reset` is available in any transition rule, actors can self-terminate directly from a state handler without root rules. When a guard returns `Reset`, the engine fires `on_exit` for every state from the current leaf up to the topmost ancestor (full LCA exit chain), then calls `on_init_entry`. This is the same code path used by `machine.reset()`.

```rust
// Supervisor's ShuttingDown state: reset when all children have shut down
const SHUTTING_DOWN_FNS: StateFns<Self> = StateFns {
    on_entry: &[stop_all_children::<R, SupervisorCtx<R>>],
    on_exit: &[],
    transitions: transitions![
        SupervisorEvent::Child(ChildLifecycleEvent::Reset { .. }) => {
            actions [record_child_reset::<R, Self>]
            guard(ctx, _results) {
                ctx.all_children_reset() => reset,
                _                        => stay,
            }
        },
        SupervisorEvent::Child(_) => stay,
    ],
};
```

The `reset` keyword in `transitions!` produces `Guard::Reset`. The full exit chain is guaranteed: `ShuttingDown::on_exit` fires, then `on_init_entry`. The runtime observes `DispatchOutcome::Reset` and emits `ChildLifecycleEvent::Reset` to the parent supervisor (if any).

---

## Topology Patterns

These patterns describe the state hierarchy of an entire blox.

### Flat FSM

All states are leaf states. No composite states. The simplest topology.

```text
[VirtualRoot]
├── StateA  (leaf)
├── StateB  (leaf)
└── StateC  (leaf)
```

Use when: the blox has a small number of states with no shared event handling.

**Example**: Pong — a single `Ready` leaf state.

---

### Composite with Shared Handler

A composite parent state handles events common to all child states. Children handle specifics.

```text
[VirtualRoot]
└── Parent  (composite)
    ├── Child1  (leaf)
    └── Child2  (leaf)
```

The parent's `transitions` slice contains rules for events that both children should respond to the same way (typically `Sink` rules to absorb unwanted events).

**Example**: `Operating` (composite) absorbs stray `PingPongMsg::Pong` while `Paused` is active. Both `Active` and `Paused` are children.

---

### Pause/Resume

A composite operating state with an `Active` leaf and a `Paused` leaf. Paused sets a timer in `on_entry`; the timer fires `Resume` which transitions back to Active.

```text
[VirtualRoot]
└── Operating  (composite)
    ├── Active  (leaf)   ← on_entry sends a ping; on Pong: guard decides Paused/Done/Active
    └── Paused  (leaf)   ← on_entry sets timer; on Resume: transition to Active
```

Use for: rate-limiting, backoff, or any pattern where the actor pauses work for a duration then resumes.

**Example**: Ping blox — pauses after round 2 for `PAUSE_DURATION_MS`.

---

### Request-Response

Send a request in `on_entry`. Wait for the reply in a transition rule. Transition based on reply content.

```text
[VirtualRoot]
└── Waiting  (leaf)  ← on_entry sends request; rule: on reply, guard decides next state
```

Use when: the blox initiates an operation and waits for a response before proceeding.

---

### Terminal with Notification

A final state where the runtime auto-notifies the supervisor via `ChildLifecycleEvent::Done`.
Set `is_terminal` on the spec so the runtime knows:

```rust
fn is_terminal(state: &MyState) -> bool {
    matches!(state, MyState::Done)
}
```

The `Done` state itself needs only an `on_entry` for any local teardown. The `transitions`
slice can be empty — all events silently drop. The runtime will send `Terminate` to
trigger `machine.reset()` when the supervisor is ready.

**Example**: Ping's `Done` state.

---

### Retry Loop

Self-transition with a counter guard. The `Active` state increments a counter in `on_entry` (since entry fires on every self-transition). The rule guards on the counter.

```rust
// State on_entry increments ctx.attempts (via action crate function)
const ACTIVE_FNS: StateFns<Self> = StateFns {
    on_entry: &[increment_attempts, send_request],
    on_exit: &[],
    transitions: transitions![
        MyMsg::Timeout(_) => {
            guard(ctx, _results) {
                ctx.attempts >= MAX_ATTEMPTS => MyState::Failed,
                _                            => MyState::Active,  // self-transition
            }
        },
    ],
};
```

Use when: the blox retries an operation a fixed number of times before giving up.

---

## Macro Quick Reference

The `transitions!` and `root_transitions!` proc macros provide concise syntax. Both support the `reset` keyword, which produces `Guard::Reset`:

```rust
// Pure Transition
transitions![MyEvent::Foo(_) => { transition MyState::Bar }]

// Sink (Absorb)
transitions![MyEvent::Foo(_) => stay,]

// Action-Then-Stay
transitions![
    MyMsg::Ping(ping) => {
        actions [Self::reply_pong_action]
        stay
    },
]

// Action-Then-Guard
transitions![
    MyMsg::Pong(pong) => {
        actions [Self::log_pong, Self::forward_ping]
        guard(ctx, results) {
            results.any_failed() => MyState::Error,
            ctx.round() >= MAX   => MyState::Done,
            _                    => MyState::Active,
        }
    },
]

// Reset (self-terminate) — available in both transitions! and root_transitions!
transitions![
    MyEvent::Shutdown(_) => reset,
]

// Action-Then-Reset-Guard
transitions![
    MyEvent::ChildDone(_) => {
        actions [Self::record_child_done]
        guard(ctx, _results) {
            ctx.all_done() => reset,
            _              => stay,
        }
    },
]
```

In `guard(ctx, results) { }` blocks, `ctx` is `&Ctx` (read-only — no mutation possible).
In `actions [fn1, fn2]` slices, each function receives `(&mut Ctx, &Event)` and returns `ActionResult`.
The `reset` outcome triggers the full LCA exit chain (leaf → root) followed by `on_init_entry`.
