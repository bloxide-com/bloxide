# Unified Lifecycle Architecture

> **When would I use this?** Use this document to understand the four-level
> lifecycle model (`reset → stop → abort → kill`), how lifecycle commands
> interact with the dispatch pipeline, how `VirtualRoot` intercepts them, and
> how supervisors observe `DispatchOutcome` events. For the supervision policy
> layer, see `08-supervision.md`; for the core HSM dispatch algorithm and
> `Init`/`VirtualRoot` mechanics, see `02-hsm-engine.md`.

Bloxide actors use a **unified lifecycle model**: every event — including
lifecycle commands such as `Start`, `Reset`, and `Stop` — flows through a single
`dispatch()` entry point and is routed by the engine's `VirtualRoot` handler.
There is no separate "lifecycle track" that bypasses dispatch. This keeps the
state machine the single source of truth for all state transitions and lets the
runtime observe every lifecycle-relevant outcome through one return type,
`DispatchOutcome`.

## Four-Level Lifecycle: `reset → stop → abort → kill`

Bloxide has four distinct lifecycle levels, ordered from gentlest to most
forceful. Each level is appropriate for a different situation:

| Level | Mechanism | Through dispatch? | Exit callbacks? | `on_init_entry`? | End state | `DispatchOutcome` | Restartable? |
|-------|-----------|-------------------|-----------------|------------------|-----------|-------------------|--------------|
| **Reset** | `LifecycleCommand::Reset` | ✅ | ✅ Full exit chain | ❌ (skips Init) | `initial_state()` — immediately operational | `Started(initial)` | ✅ immediately |
| **Stop** | `LifecycleCommand::Stop` | ✅ | ✅ Full exit chain | ✅ (cleanup) | `Init` — suspended | `Stopped` | ✅ via `Start` |
| **Abort** | `AbortCommand` on abort mailbox | ❌ (run loop breaks) | ❌ None | ❌ | Task ends (cooperative) | `Aborted` | ✅ via respawning |
| **Kill** | `R::Kill::kill(abort_handle)` | ❌ (runtime ripcord) | ❌ None | ❌ | Task gone — permanently dead | (nothing) | ❌ permanently |

### Reset — Immediate Restart

`Reset` sends the actor through its exit chain (all `on_exit` callbacks fire),
then enters the **user-defined initial operational state** (defined by
`MachineSpec::initial_state()`). The actor is immediately running again — no
separate `Start` command is needed.

**Reset skips Init entirely.** No `on_init_entry` or `on_init_exit` fires. The
`on_entry` callbacks for `initial_state()` are responsible for resetting domain
state (counters, cancel timers, etc.).

Use for: restart cycles where the actor should continue operating.

### Stop — Graceful Shutdown

`Stop` sends the actor through its exit chain (all `on_exit` callbacks fire),
calls `on_init_entry` (for resource cleanup), and leaves the actor in **Init**.
The task stays alive but suspended. Send `Start` to resume operation from
`initial_state()`.

Use for:
- Graceful shutdown (callbacks run, clean exit)
- Pausing an actor with intent to resume later
- Dynamic actors you may want to restart

### Abort — Cooperative Termination

`Abort` is sent as an `AbortCommand` on a dedicated **abort mailbox** (separate
from the lifecycle mailbox). The actor's run loop polls it alongside lifecycle
and domain mailboxes. When `AbortCommand::Abort` is received, the run loop
breaks — the task ends cooperatively. No `dispatch()` is called, no exit
callbacks fire, no `on_init_entry` fires.

The runtime synthesizes `DispatchOutcome::Aborted` and sends
`ChildLifecycleEvent::Aborted` to the supervisor. The task is ended but was not
externally destroyed — restarting requires respawning a new task.

Use for:
- Supervisor-initiated shutdown where you want the task to end but the actor
  might not be in a state where `Stop` makes sense (e.g., it's in `Init` already)
- Cases where you want the task gone but need the supervisor to know it happened
  (Kill produces no `DispatchOutcome`)

### Kill — Permanent Termination (Ripcord)

`R::Kill::kill(abort_handle)` is the external ripcord. It immediately aborts
the actor's task — works even on stuck/deadlocked actors that aren't polling
any mailbox. No callbacks, no dispatch, no mailbox. The task is permanently
dead.

The supervisor receives no `DispatchOutcome` for a killed actor; it learns the
actor is gone through the runtime's task-completion signal or its own registry
bookkeeping. `ChildPolicy::Kill` calls the ripcord directly.

Use for:
- **Unresponsive actors** — stuck in infinite loops, deadlocks, or blocking
  calls that cannot process `Stop`, `Reset`, or `Abort`. Kill forces termination
  when cooperation is not possible.

Cooperative cleanup of stopped or aborted actors is handled by the `Stop` and
`Abort` paths respectively — `Kill` is not used for those.

### KillCapability

`KillCapability` is a runtime-facing capability trait (Tier 2), not a
blox-facing trait. Only supervisors and the wiring layer hold kill handles;
actors never see `KillCapability`. It exists solely to back `ChildPolicy::Kill`
— the ripcord-only, forcible-termination path. Cooperative cleanup is handled
by `ChildPolicy::Abort` (which sends an `AbortCommand` and lets the child
self-terminate); `KillCapability` is not involved in that path.

```rust
// In bloxide-core/src/capability.rs
pub trait KillCapability<R: BloxRuntime> {
    type Handle: Clone + Send + 'static;
    fn kill(handle: Self::Handle);
}
```

The runtime provides two implementations:

- **`Kill`** — for dynamic runtimes (Tokio). `Handle = R::AbortHandle`,
  `kill` calls `R::abort(handle)`, which aborts the spawned task.
- **`NoKill`** — for static runtimes (Embassy). `Handle = ()` (a ZST), `kill`
  is a no-op, because static actors cannot be aborted at runtime.

The supervisor stores the concrete abort handle per child in its `ChildEntry`
registry (the `abort_ref` field) — not a trait object. There is no
`Arc<dyn KillCapability>` and no dynamic dispatch: the abort handle is a
concrete, clonable value supplied by the runtime, and the supervisor invokes
`R::Kill::kill(abort_handle)` only when its policy is `ChildPolicy::Kill` and
immediate, non-cooperative cleanup is required.

## Core Concept: One Dispatch Pipeline

All mailboxes (lifecycle and domain) are polled together by the `Mailboxes`
trait, which returns a unified event stream. Each event is handed to
`StateMachine::dispatch()`. Inside `dispatch`, the engine checks whether the
event carries a `LifecycleCommand` (via the `LifecycleEvent::as_lifecycle_command`
trait method). If it does, `VirtualRoot` handles it before any user-declared
state ever sees it. Otherwise the event is routed through the user's state
handler tables, bubbling from the active leaf up to `VirtualRoot`.

```
All mailboxes (lifecycle + domain)
        │
        ▼
   dispatch(event)
        │
        ├── event carries LifecycleCommand?  ──►  VirtualRoot handles it
        │                                              (Start / Reset / Stop / Ping)
        │
        └── domain event  ──►  active leaf → ancestors → VirtualRoot
                               (user handler tables)
```

`Abort` and `Kill` are **not** `LifecycleCommand` variants — they bypass
dispatch entirely. `Abort` is received on a separate abort mailbox and breaks
the run loop. `Kill` is a runtime capability call that destroys the task.

`VirtualRoot` is engine-implicit — it is not a member of the user's `State`
enum. It exists solely as the top of the state hierarchy for LCA computation
and as the place where lifecycle commands are intercepted. Top-level user
states return `None` from `parent()`, which makes them direct children of
`VirtualRoot`.

## Init as an Implicit Leaf

`Init` is an engine-implicit leaf state, separate from the user's `State` enum.
A freshly constructed `StateMachine` begins in `Init` **silently** — no
`on_entry` callbacks fire at construction time. `on_init_entry` fires only when
the machine enters `Init` via `Stop` (for resource cleanup). It does **not**
fire on `Reset` (which skips Init entirely) or `Abort`/`Kill` (which bypass
dispatch).

The current state is tracked by the `MachineState<S>` wrapper, which is the
moral equivalent of `Option<S>` but semantically distinguishes the engine's
implicit `Init` from any user-declared state that happens to be named "Init":

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineState<S> {
    /// Implicit Init state — machine is in lifecycle wait state.
    Init,
    /// One of the user's declared operational states.
    State(S),
}
```

Users may declare their own domain state also named `Init` with no conflict,
because the engine's `Init` is never a variant of the user's enum.

### State Tree

```
VirtualRoot (implicit, not entered/exited — used for LCA and lifecycle intercept)
    │
    ├── Init (implicit leaf, auto-generated)
    │       on_entry: S::on_init_entry   (fires on Stop only)
    │       on_exit:  S::on_init_exit    (fires on Start)
    │       transitions: [* => stay]      (catch-all for domain events)
    │
    ├── Waiting   (user-declared leaf, returned by initial_state())
    ├── Running   (user-declared leaf)
    └── Done      (user-declared leaf)
```

While in `Init`, all non-lifecycle domain events are **silently dropped**:
`Init`'s auto-generated handler table is a catch-all that maps every domain
event to `Stay`. Lifecycle commands, however, are still processed at
`VirtualRoot` regardless of current state — so a machine sitting in `Init`
still responds to `Start`, `Reset`, `Stop`, and `Ping`.

## Lifecycle Commands and the Mailbox Event Wrapper

`LifecycleCommand` is the set of commands that `VirtualRoot` knows how to
handle:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleCommand {
    /// Transition from Init to the operational initial state.
    Start,
    /// Go directly to initial_state() — immediately operational, skips Init.
    Reset,
    /// Transition to Init, report Stopped. Actor stays in Init.
    Stop,
    /// Health check — respond with Alive.
    Ping,
}
```

`Abort` and `Kill` are **not** `LifecycleCommand` variants. `Abort` is an
`AbortCommand` sent on a separate abort mailbox (polled by the run loop but
not dispatched through the state machine). `Kill` is a runtime capability
call, not a message at all.

### Wrapping Lifecycle Commands in the Event Enum

Lifecycle commands are not dispatched as a bare enum. Each actor's event type
wraps `LifecycleCommand` in one of its own variants and implements the
`LifecycleEvent` trait so the engine can recognise it:

```rust
pub trait LifecycleEvent: EventTag {
    /// Returns the lifecycle command if this event wraps one.
    /// Returns None for domain events.
    fn as_lifecycle_command(&self) -> Option<LifecycleCommand> { None }
}
```

`dispatch()` calls `as_lifecycle_command()` on every event. When it returns
`Some(cmd)`, the engine routes the event to its `VirtualRoot` lifecycle handler
and never consults the user's state handler tables for that event. When it
returns `None`, the event is treated as a domain event and flows through the
normal leaf-to-root handler lookup.

This wrapper design is what makes the lifecycle model "unified": lifecycle
commands travel through the same mailbox stream and the same `dispatch()` call
as domain events, rather than being intercepted by the runtime before the state
machine ever sees them.

### Mailbox Priority Ordering

The `Mailboxes` trait polls its constituent streams in **priority order**: the
first stream in the tuple is always polled first, and when it has a message
that message is returned immediately without checking later streams. The
convention is to place the lifecycle mailbox at index 0 so that lifecycle
commands are always drained before domain events:

```rust
// Convention for supervised actors:
type Mailboxes<R> = (
    R::Stream<LifecycleCommand>, // index 0 — highest priority
    R::Stream<DomainMsg>,        // index 1 — domain
);
```

This guarantees that a `Stop` or `Reset` sent while domain messages are
backlogged is still observed promptly, because `dispatch()` will see the
lifecycle command before any queued domain event.

## Guard::Reset and Guard::Fail

State handler rules return a `Guard` outcome. In addition to `Transition` and
`Stay`, the engine recognises two lifecycle-affecting outcomes:

```rust
pub enum Guard<S: MachineSpec> {
    Transition(LeafState<S::State>),
    Stay,
    /// Self-reset: go directly to initial_state(), skipping Init entirely.
    /// Fires full exit chain + entry chain for initial_state().
    /// Does NOT call on_init_entry. Returns Started.
    Reset,
    /// Go to error_state(), report Failed to supervisor.
    /// Fires full exit chain + entry chain for error_state().
    /// Does NOT call on_init_entry. Returns Failed.
    Fail,
}
```

### Guard::Reset

When a state handler returns `Reset`, the engine:

1. Runs `on_exit` for every state from the current leaf up to the root (the
   full exit chain).
2. Runs `on_entry` for the `initial_state()` path (root-to-leaf).
3. Sets the current state to `initial_state()`.
4. Returns `DispatchOutcome::Started(initial_state)`.

**Reset skips Init entirely.** No `on_init_entry` fires. The actor is
immediately operational.

### Guard::Fail

When a state handler returns `Fail`, the engine:

1. Runs `on_exit` for every state from the current leaf up to the root (the
   full exit chain).
2. Runs `on_entry` for the entry chain of `MachineSpec::error_state()` (which
   defaults to `initial_state()` if not overridden).
3. Sets the current state to `MachineState::State(error_state())`.
4. Returns `DispatchOutcome::Failed` from `dispatch()`.

`on_init_entry` is **not** called — `Fail` jumps directly to `error_state()`,
skipping `Init` entirely. The supervisor observes `Failed` and applies its
`ChildPolicy` — typically restarting the actor by sending `Start`, or marking
it permanently failed.

## Supervisor Observation of DispatchOutcome

`dispatch()` returns a `DispatchOutcome`, which the runtime actor loop forwards
to the supervisor as a `ChildLifecycleEvent`. This is how the supervisor learns
about lifecycle transitions without being coupled to the actor's event types:

```rust
pub enum DispatchOutcome<State> {
    /// No rule matched anywhere (event bubbled to VirtualRoot with no match).
    NoRuleMatched,
    /// Rule matched but guard returned Stay.
    HandledNoTransition,
    /// Transition occurred to a user state.
    Transition(MachineState<State>),
    /// Left Init via Start, OR Reset went directly to initial_state().
    Started(MachineState<State>),
    /// Transitioned to terminal state.
    Done(MachineState<State>),
    /// Actor failed via Guard::Fail (jumped to error_state()).
    Failed,
    /// Actor stopped to Init via LifecycleCommand::Stop.
    Stopped,
    /// Actor aborted cooperatively via AbortCommand on abort mailbox.
    Aborted,
    /// Actor responded to Ping.
    Alive,
}
```

The runtime actor loop maps each outcome to a `ChildLifecycleEvent` and sends
it to the supervisor's mailbox. Only lifecycle-relevant outcomes produce a
notification — `NoRuleMatched`, `HandledNoTransition`, and `Transition` are
internal state-machine events that the supervisor does not need to see:

| `DispatchOutcome`        | `ChildLifecycleEvent` | Supervisor Action                            |
|---------------------------|-----------------------|----------------------------------------------|
| `Started(_)`              | `Started`             | Record child as running (covers both Start and Reset) |
| `Done(_)`                 | `Done`                | Apply `ChildPolicy` (restart, stop, abort, or kill) |
| `Failed`                  | `Failed`              | Apply `ChildPolicy` (restart, stop, abort, or kill) |
| `Stopped`                 | `Stopped`             | Record child as suspended in Init            |
| `Aborted`                 | `Aborted`             | `record_aborted()` — child permanently done  |
| `Alive`                   | `Alive`               | Record child as responsive                   |
| `NoRuleMatched`           | —                     | (not forwarded)                              |
| `HandledNoTransition`     | —                     | (not forwarded)                              |
| `Transition(_)`           | —                     | (not forwarded)                              |

Note: `Reset` no longer has a dedicated `DispatchOutcome` variant. Reset
returns `Started(initial_state)`, which the runtime maps to
`ChildLifecycleEvent::Started`. The supervisor sees `Started` and knows the
child is operational — no separate `Start` is needed.

Because the supervisor is itself a state machine actor, it handles these
`ChildLifecycleEvent` messages through its own `dispatch()` pipeline and state
handler tables — the same unified mechanism as every other actor. There is no
special "supervisor channel" that bypasses dispatch: child lifecycle events
arrive as ordinary domain events on the supervisor's mailbox and are routed
through the supervisor's handler tables, where they trigger policy actions such
as sending `Start` to restart a failed child, sending `AbortCommand::Abort` for
cooperative self-termination (`ChildPolicy::Abort`), or invoking
`R::Kill::kill(abort_handle)` to forcibly remove one (`ChildPolicy::Kill`).

This observer model means actors have **zero knowledge of their supervisor**.
There is no `supervisor_ref` in actor context, no lifecycle messages in the
actor's event enum beyond the wrapper for `LifecycleCommand`, and no root rules
for `Reset`/`Stop`/`Ping` in the actor's own handler tables. The actor simply
runs its state machine and returns `DispatchOutcome` values; the supervisor
watches those outcomes and decides what to do.

## `ChildLifecycleEvent`

Defined in `bloxide-core/src/lifecycle.rs`. The runtime generates these
automatically by observing `DispatchOutcome` — no actor code sends them.

```rust
pub enum ChildLifecycleEvent {
    Started { child_id: ActorId },  // child exited Init or was Reset (now operational)
    Done    { child_id: ActorId },  // child entered a terminal state (is_terminal)
    Failed  { child_id: ActorId },  // child entered an error state (is_error)
    Stopped { child_id: ActorId },  // child was Stopped, now in Init (suspended)
    Aborted { child_id: ActorId },  // child was Aborted, task has ended (cooperative)
    Alive   { child_id: ActorId },  // child responded to Ping (healthy)
}
```

`is_error` takes precedence: if both `is_error` and `is_terminal` return `true`
for the same state, only `Failed` is reported.

## Related Docs

- **HSM dispatch algorithm** → `02-hsm-engine.md`
- **Supervision policies** → `08-supervision.md`
- **Spawn architecture** → `18-spawn-architecture.md`
