# Handler and Topology Patterns

> **When would I use this?** Use this document when implementing event
> handlers, transition rules, or state topologies for a blox. For the
> dispatch algorithm and lifecycle handling, see `01-hsm-engine.md`.

> Transition rules are declared declaratively in `blox.toml` via
> `[[topology.transitions]]`, and `bloxide-codegen` emits raw
> `StateRule { ... }` struct literals from those entries — no proc macro is
> involved. The code blocks below show the TOML syntax for each pattern. See
> `spec/architecture/15-blox-toml-source-of-truth.md` for the current
> TOML schema and `QUICK_REFERENCE.md` → "Declarative Transitions
> (blox.toml)" for a worked example.

This document defines named, reusable patterns for event handler rules and blox state topologies. When building a blox, refer to these patterns by name in your spec and implementation. AI agents should use these patterns as the canonical vocabulary for describing blox behavior.

---

## Handler Model

Each state's `StateFns` contains a `transitions` slice of `TransitionRule` structs. The engine evaluates rules in declaration order; the first matching rule wins. **Bubbling is implicit**: if no rule matches in the current state, the engine moves to the parent and evaluates its rules. No manual "return Parent" — bubbling happens automatically.

```rust
pub struct TransitionRule<S: MachineSpec, G> {
    pub event_tag: u8,                                              // fast pre-filter
    pub matches: fn(&S::Event) -> bool,                             // does this rule apply?
    pub actions: &'static [fn(&mut S::Ctx, &S::Event) -> ActionResult], // side effects (mutable context)
    pub guard: fn(&S::Ctx, &ActionResults, &S::Event) -> G,        // transition decision (read-only)
}
```

The ordering enforces the invariant: **actions always precede the guard**. The borrow checker enforces that **guards cannot mutate context** (`&Ctx`, not `&mut Ctx`). Each action returns `ActionResult`; the engine collects them into `ActionResults` before calling the guard. Guards receive `&ActionResults` to inspect failures (e.g. `results.any_failed()`).

Root-level rules use `StateRule<S>`, the same type as state-level rules — both return `Decision<S>`. All six `Decision` variants (`Transition`, `Stay`, `Reset`, `Stop`, `Done`, `Fail`) are available at all levels — state rules and root rules have identical decision capabilities.

---

## Per-Rule Handler Patterns

These patterns describe a single `TransitionRule` within a state's `transitions` slice.

### 1. Pure Transition

No side effects. The event itself is the complete signal.

```toml
[[topology.transitions]]
state = "Foo"
event = "MyEvent::Go(_)"
target = "Bar"
```

Use when: the event variant alone determines the next state, with no context inspection needed.

**Example**: `Ping`'s `Paused` state receives `PingPongMsg::Resume(_)` and always transitions to `Active`.

---

### 2. Sink (Absorb)

Match and absorb the event. Prevents bubbling to the parent state.

```toml
[[topology.transitions]]
state = "Operating"
event = "MyEvent::Foo(_)"
target = "stay"
```

Use when: a parent composite state should silence an event that a child state doesn't handle.

**Example**: `Operating` absorbs stray `PingPongMsg::Pong` events while `Paused` is active.

---

### 3. Action-Then-Stay

Side effects with no state change. The event is handled locally.

```toml
[[topology.transitions]]
state = "Ready"
event = "PingPongMsg::Ping(ping)"
actions = ["send_pong"]
target = "stay"
```

Use when: an event triggers side effects but no state change (e.g., fire-and-forget response).

**Example**: `Pong`'s `Ready` state receives `PingPongMsg::Ping`, sends back a Pong reply, stays in `Ready`.

---

### 4. Action-Then-Guard

Side effects followed by a conditional transition. The most common pattern.

```toml
[[topology.transitions]]
state = "Active"
event = "PingPongMsg::Pong(pong)"
target = "Active"  # fallback when no guard condition matches
actions = ["forward_ping"]

  [[topology.transitions.guards]]
  condition = "results.any_failed()"
  target = "Error"

  [[topology.transitions.guards]]
  condition = "ctx.round >= MAX_ROUNDS as u32"
  target = "done"

  [[topology.transitions.guards]]
  condition = "ctx.round == PAUSE_AT_ROUND as u32"
  target = "Paused"
```

Use when: context is updated or messages are sent, and the resulting context (or action results) determines the next state.

**Example**: `Ping`'s `Active` state sends the next ping (`forward_ping`), then guards on `results.any_failed()` first (error priority), then round counters to decide between `Error`, `Stop` (self-suspend), `Paused`, or self-transition back to `Active`.

---

### 5. Pure Guard

No side effects. The guard reads context to decide the transition.

```toml
[[topology.transitions]]
state = "Waiting"
event = "MyEvent::Tick(_)"
target = "stay"  # fallback when no guard condition matches

  [[topology.transitions.guards]]
  condition = "ctx.deadline_elapsed"
  target = "Timeout"
```

Use when: context state (not the event) determines the transition, and the event is merely a trigger.

---

### 6. Bubble (Implicit)

No rule is needed. When no rule matches in the current state, the engine automatically bubbles the event to the parent state (moves the cursor up and evaluates that state's rules).

The "Bubble" pattern is the **absence of a rule**. Do not add a catch-all rule that manually returns a parent — bubbling is implicit. Simply omit a rule for the event variant.

> See `AGENTS.md` invariant #8 for the formal constraint: never add a catch-all
> rule that manually returns a parent; bubbling is implicit.

Use when: a leaf state does not handle an event and wants its parent (or root) to handle it.

**Example**: `Done` state (if declared) has an empty `transitions: &[]` — all events bubble to root, which silently drops them (or handles any root rules you define). Actors that self-suspend via `Decision::Stop` do not need a `Done` state at all.

---

## Root Rule Patterns

Root rules use `StateRule<S>` with `Decision` — all six variants (`Transition`, `Stay`, `Reset`, `Stop`, `Done`, `Fail`). Root rules are the same type as state-level rules — `root_transitions()` returns `&'static [StateRule<Self>]`.

> **Canonical source for lifecycle handling**: `spec/architecture/01-hsm-engine.md`
> documents how lifecycle commands (Start, Reset, Stop, Ping) flow through
> `dispatch()` at the VirtualRoot level. Supervised actors return `&[]` from
> `root_transitions()` — lifecycle is handled by engine defaults.

`root_transitions()` has a default empty implementation (`&[]`) and is **optional** for most actors. Override it only if you need fallback rules that apply when an event bubbles past all user-declared states:

```rust
// Default — no override needed for most actors:
fn root_transitions() -> &'static [StateRule<Self>] { &[] }
```

### `reset` in State-Level Transitions

Since `Decision::Reset` is available in any transition rule, actors can self-restart directly from a state handler without root rules. When a rule returns `Reset`, the engine performs an LCA-based `change_state` to `initial_state()`: `on_exit` fires for every state from the current leaf up to (but not including) the lowest common ancestor with the `initial_state()` path, then `on_entry` fires down to `initial_state()`. Reset skips Init — `on_init_entry` does NOT fire. The runtime observes `DispatchOutcome::Started(initial_state)` and emits `ChildLifecycleEvent::Started` to the supervisor.

```toml
# Supervisor's ShuttingDown state: reset when all children have stopped
[[topology.transitions]]
state = "ShuttingDown"
event = "SupervisorEvent::Child(ChildLifecycleEvent::Stopped { .. })"
target = "stay"  # fallback
actions = ["record_stopped"]

  [[topology.transitions.guards]]
  condition = "ctx.all_children_stopped()"
  target = "reset"

[[topology.transitions]]
state = "ShuttingDown"
event = "SupervisorEvent::Child(_)"
target = "stay"
```

The `reset` target in a `[[topology.transitions]]` entry produces `Decision::Reset`. The LCA-based `change_state` applies: `ShuttingDown` and `Running` share no user ancestor, so `ShuttingDown::on_exit` fires (full exit chain in this topology), then `on_entry` for `initial_state()` (Running). The runtime observes `DispatchOutcome::Started(Running)` and emits `ChildLifecycleEvent::Started`. The supervisor self-restarts by re-entering `Running` and calling `start_children` to send `Start` to all children.

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
    ├── Active  (leaf)   ← on_entry sends a ping; on Pong: guard decides Paused/Stop/Active
    └── Paused  (leaf)   ← on_entry sets timer; on Resume: transition to Active
```

Use for: rate-limiting, backoff, or any pattern where the actor pauses work for a duration then resumes.

**Example**: Ping blox — pauses after round 2 for `2000 + round × 500` ms (via `schedule_resume`).

---

### Request-Response

Send a request in `on_entry`. Wait for the reply in a transition rule. Transition based on reply content.

```text
[VirtualRoot]
└── Waiting  (leaf)  ← on_entry sends request; rule: on reply, guard decides next state
```

Use when: the blox initiates an operation and waits for a response before proceeding.

---

### Self-Suspend via Decision::Stop

An actor that has completed its work can self-suspend by returning `Decision::Stop` from a transition rule. The engine fires the full exit chain, calls `on_init_entry` (for cleanup), sets the state to `Init`, and returns `DispatchOutcome::Stopped`. The runtime notifies the supervisor via `ChildLifecycleEvent::Stopped`.

```toml
[[topology.transitions]]
state = "Active"
event = "PoolMsg::WorkDone(_)"
target = "stay"  # fallback
actions = ["handle_work_done"]

  [[topology.transitions.guards]]
  condition = "ctx.pending == 0"
  target = "stop"
```

The `stop` target in a `[[topology.transitions]]` entry produces `Decision::Stop`. The full exit chain is guaranteed, then `on_init_entry` fires for cleanup. The actor sits suspended in `Init`; for supervised actors the run loop stays alive (`exit_on_stop = false`) and the supervisor sees `Stopped` and can later send `Start` to resume — for root/unsupervised/bare actors (`exit_on_stop = true`) the run loop exits instead.

**Example**: Pool's `Active` state returns `Decision::Stop` when `pending == 0`.

---

### Retry Loop

Self-transition with a counter guard. The `Active` state increments a counter in `on_entry` (since entry fires on every self-transition). The rule guards on the counter.

```toml
# State on_entry increments ctx.attempts (via context crate function)
[[topology.transitions]]
state = "Active"
event = "MyMsg::Timeout(_)"
target = "Active"  # fallback: self-transition retries

  [[topology.transitions.guards]]
  condition = "ctx.attempts >= MAX_ATTEMPTS"
  target = "Failed"
```

Use when: the blox retries an operation a fixed number of times before giving up.

---

## Declarative Transition Syntax (blox.toml)

The patterns above are expressed in `blox.toml` as `[[topology.transitions]]` entries. The codegen emits `StateRule` struct literals directly — no proc macro is involved. Both state-level and root-level rules support the `reset` target, which produces `Decision::Reset`:

```toml
# Pure Transition
[[topology.transitions]]
state = "Foo"
event = "MyEvent::Go(_)"
target = "Bar"

# Sink (Absorb)
[[topology.transitions]]
state = "Operating"
event = "MyEvent::Foo(_)"
target = "stay"

# Action-Then-Stay
[[topology.transitions]]
state = "Ready"
event = "MyMsg::Ping(ping)"
target = "stay"
actions = ["send_pong"]

# Action-Then-Guard — transition-level target is the fallback
[[topology.transitions]]
state = "Active"
event = "MyMsg::Pong(pong)"
target = "Active"  # fallback when no guard condition matches
actions = ["forward_ping"]

  [[topology.transitions.guards]]
  condition = "results.any_failed()"
  target = "Error"

  [[topology.transitions.guards]]
  condition = "ctx.round >= MAX"
  target = "stop"

# Reset (self-restart) — available in both state-scope and root-scope rules
[[topology.transitions]]
state = "Running"
event = "MyEvent::Shutdown(_)"
target = "reset"

# Action-Then-Reset-Guard — explicit wildcard guard as the last arm
[[topology.transitions]]
state = "ShuttingDown"
event = "MyEvent::ChildDone(_)"
target = "stay"
actions = ["record_child_done"]

  [[topology.transitions.guards]]
  condition = "ctx.all_done"
  target = "reset"

  [[topology.transitions.guards]]
  condition = "_"
  target = "stay"
```

Every `[[topology.transitions]]` entry requires `state`, `event`, and `target`.
When `guards` are present, the transition-level `target` is the fallback arm
(taken when no guard condition matches); a last guard with `condition = "_"` is
an explicit wildcard fallback (see `crates/bloxes/counter/blox.toml`).
In guard conditions, `ctx` is `&Ctx` (read-only — direct field access, no mutation)
and `results` is `&ActionResults`.
In `actions = ["fn1", "fn2"]` lists, each function receives `(&mut Ctx, &Event)` and returns `ActionResult`.
The `reset` target performs an LCA-based `change_state` to `initial_state()`: `on_exit` fires from the current leaf up to (not including) the lowest common ancestor, then `on_entry` fires down to `initial_state()`; Init is skipped. The `stop` target triggers the full exit chain plus `on_init_entry` (the actor enters Init).

## Related Docs

- **Action functions** → `spec/architecture/05-actions.md`
- **Declarative transitions (blox.toml)** → `QUICK_REFERENCE.md` → "Declarative Transitions (blox.toml)" and `spec/architecture/15-blox-toml-source-of-truth.md`
- **Dispatch algorithm and lifecycle** → `spec/architecture/01-hsm-engine.md`
- **Examples in practice** → `spec/bloxes/ping.md`, `spec/bloxes/pong.md`
