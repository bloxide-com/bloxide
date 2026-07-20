# Unified Lifecycle Architecture

> **When would I use this?** Use this document to understand how lifecycle
> commands (Start/Reset/Stop/Kill) interact with the dispatch pipeline, how
> `VirtualRoot` intercepts them, and how supervisors observe `DispatchOutcome`
> events. For the supervision policy layer, see `08-supervision.md`; for the
> core HSM dispatch algorithm and `Init`/`VirtualRoot` mechanics, see
> `02-hsm-engine.md`.

Bloxide actors use a **unified lifecycle model**: every event — including
lifecycle commands such as `Start`, `Reset`, and `Stop` — flows through a single
`dispatch()` entry point and is routed by the engine's `VirtualRoot` handler.
There is no separate "lifecycle track" that bypasses dispatch. This keeps the
state machine the single source of truth for all state transitions and lets the
runtime observe every lifecycle-relevant outcome through one return type,
`DispatchOutcome`.

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

`VirtualRoot` is engine-implicit — it is not a member of the user's `State`
enum. It exists solely as the top of the state hierarchy for LCA computation
and as the place where lifecycle commands are intercepted. Top-level user
states return `None` from `parent()`, which makes them direct children of
`VirtualRoot`.

## Init as an Implicit Leaf

`Init` is an engine-implicit leaf state, separate from the user's `State` enum.
A freshly constructed `StateMachine` begins in `Init` **silently** — no
`on_entry` callbacks fire at construction time. `on_init_entry` fires only when
the machine *re-enters* `Init` as the result of a `Reset`, `Fail`, or `Stop`.
Likewise, `on_init_exit` fires only when `Init` is left via the `Start`
command.

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
    │       on_entry: S::on_init_entry   (fires on Reset / Fail / Stop)
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
    /// Transition to Init, report Reset. Actor can be restarted.
    Reset,
    /// Transition to Init, report Stopped. Actor stays in Init.
    Stop,
    /// Health check — respond with Alive.
    Ping,
}
```

`Kill` is **not** a `LifecycleCommand`. It is a runtime capability, not a
message (see [Kill vs Stop vs Reset](#kill-vs-stop-vs-reset) below).

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

## Guard::Fail and Error Recovery

State handler rules return a `Guard` outcome. In addition to `Transition` and
`Stay`, the engine recognises two lifecycle-affecting outcomes:

```rust
pub enum Guard<S: MachineSpec> {
    Transition(LeafState<S::State>),
    Stay,
    /// Exit to implicit Init, report Reset to supervisor.
    Reset,
    /// Exit to implicit Init, report Failed to supervisor.
    Fail,
}
```

`Guard::Fail` is the error-propagation outcome. When a state handler returns
`Fail`, the engine:

1. Runs `on_exit` for every state from the current leaf up to the topmost
   ancestor (the full exit chain).
2. Calls `MachineSpec::on_init_entry` — giving the actor a chance to release
   resources and reset domain state.
3. Sets the current state to `MachineState::Init`.
4. Returns `DispatchOutcome::Failed` from `dispatch()`.

The supervisor observes that `Failed` outcome (see
[Supervisor Observation](#supervisor-observation-of-dispatchoutcome) below) and
applies its `ChildPolicy` — typically restarting the actor by sending `Start`,
or marking it permanently failed.

`Guard::Reset` follows the same exit-chain and `on_init_entry` mechanics but
reports `DispatchOutcome::Reset`, which the supervisor treats as a normal
restart cycle rather than a failure.

## Kill vs Stop vs Reset

Three mechanisms can move an actor out of its current operational state. They
differ in whether they go through `dispatch()`, whether callbacks fire, and
whether the actor can be restarted.

| Action | Mechanism                                         | Actor Still Exists? | Task Running? | Supervisor Sees |
|--------|---------------------------------------------------|---------------------|---------------|-----------------|
| Reset  | `LifecycleCommand` → `dispatch()` → Init          | Yes                 | Yes           | `Reset`         |
| Stop   | `LifecycleCommand` → `dispatch()` → Init          | Yes                 | Yes           | `Stopped`       |
| Kill   | `KillCapability::kill(handle)` (bypasses dispatch) | No                  | No            | (nothing — actor is gone) |

`Reset` and `Stop` both transition the actor to `Init` and fire the full exit
chain plus `on_init_entry`. The difference is the outcome the supervisor
observes: `Reset` signals a restart cycle (the actor is expected to continue
operating), while `Stop` signals a graceful shutdown (the actor sits suspended
in `Init` until a `Start` arrives).

`Kill` is fundamentally different: it is a **runtime capability**, not a
message. It immediately aborts the actor's task. No `on_exit` or `on_init_entry`
callbacks fire — the task is dropped in place. The actor is permanently dead
and cannot be restarted. The supervisor receives no `DispatchOutcome` for a
killed actor; it learns the actor is gone through the runtime's task-completion
signal or its own registry bookkeeping.

### KillCapability

`KillCapability` is a runtime-facing capability trait (Tier 2), not a
blox-facing trait. Only supervisors and the wiring layer hold kill handles;
actors never see `KillCapability`.

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

The supervisor stores the concrete `TaskHandle` per child in its `ChildEntry`
registry — not a trait object. There is no `Arc<dyn KillCapability>` and no
dynamic dispatch: the kill handle is a concrete, clonable value supplied by the
runtime, and the supervisor invokes `KillCapability::kill(handle)` when its
policy dictates immediate cleanup.

`Kill` has two purposes:

1. **Unresponsive actors** — actors stuck in infinite loops, deadlocks, or
   blocking calls that cannot process `Stop` or `Reset`. Kill forces
   termination when cooperation is not possible.
2. **Cleanup of stopped actors** — an actor was stopped (now in `Init`), but
   the supervisor wants to free its resources immediately rather than keep it
   suspended.

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
    /// Left Init via Start command.
    Started(MachineState<State>),
    /// Transitioned to terminal state.
    Done(MachineState<State>),
    /// Actor reset to Init via Guard::Reset.
    Reset,
    /// Actor failed to Init via Guard::Fail or entered error state.
    Failed,
    /// Actor stopped to Init via LifecycleCommand::Stop.
    Stopped,
    /// Actor responded to Ping.
    Alive,
}
```

The runtime actor loop maps each outcome to a `ChildLifecycleEvent` and sends
it to the supervisor's mailbox. Only lifecycle-relevant outcomes produce a
notification — `NoRuleMatched`, `HandledNoTransition`, and `Transition` are
internal state-machine events that the supervisor does not need to see:

| `DispatchOutcome`        | `ChildLifecycleEvent` | Supervisor Action |
|---------------------------|-----------------------|-------------------|
| `Started(_)`              | `Started`             | Record child as running |
| `Done(_)`                 | `Done`                | Apply `ChildPolicy` (restart or stop) |
| `Reset`                   | `Reset`               | Apply restart policy |
| `Failed`                  | `Failed`              | Apply `ChildPolicy` (restart or stop) |
| `Stopped`                 | `Stopped`             | Record child as suspended in Init |
| `Alive`                   | `Alive`               | Record child as responsive |
| `NoRuleMatched`           | —                     | (not forwarded) |
| `HandledNoTransition`     | —                     | (not forwarded) |
| `Transition(_)`           | —                     | (not forwarded) |

Because the supervisor is itself a state machine actor, it handles these
`ChildLifecycleEvent` messages through its own `dispatch()` pipeline and state
handler tables — the same unified mechanism as every other actor. There is no
special "supervisor channel" that bypasses dispatch: child lifecycle events
arrive as ordinary domain events on the supervisor's mailbox and are routed
through the supervisor's handler tables, where they trigger policy actions such
as sending `Start` to restart a failed child or invoking `KillCapability::kill`
to permanently remove one.

This observer model means actors have **zero knowledge of their supervisor**.
There is no `supervisor_ref` in actor context, no lifecycle messages in the
actor's event enum beyond the wrapper for `LifecycleCommand`, and no root rules
for `Reset`/`Stop`/`Ping` in the actor's own handler tables. The actor simply
runs its state machine and returns `DispatchOutcome` values; the supervisor
watches those outcomes and decides what to do.
