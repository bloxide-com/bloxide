# Effects and Capabilities

> **When would I use this?** Use this document when implementing timer
> patterns, understanding how capabilities flow through the action layer, or
> working with `TestRuntime` and `VirtualClock` for deterministic testing.
> For the two-tier trait system overview, see `00-layered-architecture.md`.

The capability system is how Bloxide exposes runtime effects (timers, I/O, storage,
network) to domain code without coupling blox crates to any specific runtime.

## Purpose

A blox describes _what_ to do in response to events. The runtime describes _how_
to carry out side effects. Keeping these concerns separate is what makes a blox
simultaneously runnable on Embassy, testable without an executor, and portable to
future runtimes.

## Design Philosophy

Effects are modeled through the **two-tier trait system** (see [00-layered-architecture.md](00-layered-architecture.md) for the full reference), not as orthogonal HSM state regions or background threads. The HSM engine remains pure: it calls `on_entry`, `on_exit`, and `actions` functions and updates the current state. It never calls runtime methods directly. All side effects originate from user-written functions in those callbacks.

Blox crates are generic over a single Tier 1 trait: `R: BloxRuntime`. All additional capabilities (timers, supervision) are exposed through **standard library crates** that define accessor traits, action functions, and messages — never as additional runtime bounds on the blox.

```
┌─────────────────────────────────────────────────────────────┐
│                          Blox Crate                         │
│  on_entry / on_exit / actions  ──calls──▶  action functions │
│       (pure logic + guard)       set_timer(ctx, ...)        │
│                                  cancel_timer(ctx, ...)     │
└─────────────────────────────────────────────────────────────┘
                            │
          Tier 1: R: BloxRuntime only
                            │
┌─────────────────────────────────────────────────────────────┐
│               Standard Library Crates (Layer 2)             │
│  bloxide-timer: TimerCommand, TimerQueue, HasTimerRef<R>,   │
│                 set_timer(), cancel_timer()                  │
│  bloxide-supervisor: LifecycleCommand, ChildGroup, ...      │
└─────────────────────────────────────────────────────────────┘
                            │
          Tier 2: TimerService, SupervisedRunLoop
                            │
┌─────────────────────────────────────────────────────────────┐
│                       Runtime Crate                          │
│       EmbassyRuntime   or   TestRuntime   or   TokioRuntime │
│       impl BloxRuntime + TimerService + SupervisedRunLoop    │
└─────────────────────────────────────────────────────────────┘
```

## Timer-as-Service Pattern (`bloxide-timer`)

Timers are a **standard library crate** with both blox-facing and runtime-facing components:

### Blox-facing (used by blox crates)

```rust
/// Unique identifier for a pending timer.
pub struct TimerId(usize);

/// Command sent to the timer service.
pub enum TimerCommand {
    Set { id: TimerId, after_ms: u64, deliver: Box<dyn FnOnce() + Send> },
    Cancel { id: TimerId },
    /// Shut down the timer service. All pending expired timers fire their callbacks
    /// and the service loop exits. Used during orderly shutdown in tests.
    Shutdown,
}

/// Queue of pending timer commands. Held by contexts that need timers.
pub struct TimerQueue { /* ... */ }

/// Accessor trait for contexts that hold a timer ref.
pub trait HasTimerRef<R: BloxRuntime> {
    fn timer_ref(&self) -> &ActorRef<TimerCommand, R>;
}
```

Action functions for blox code:

```rust
/// Schedule `event` to be delivered to `target` after `after_ms` milliseconds.
/// Returns a `TimerId` for cancellation.
pub fn set_timer<R, C, M>(
    ctx: &C,
    after_ms: u64,
    target: &ActorRef<M, R>,
    event: M,
) -> TimerId
where
    R: BloxRuntime,
    C: HasSelfId + HasTimerRef<R>,
    M: Send + 'static;

/// Cancel a pending timer.
pub fn cancel_timer<R, C>(
    ctx: &C,
    id: TimerId,
)
where
    R: BloxRuntime,
    C: HasSelfId + HasTimerRef<R>;
```

### Runtime-facing (implemented by runtime crates)

```rust
/// Service trait that runtimes implement to bridge TimerQueue to native timers.
pub trait TimerService: BloxRuntime {
    // Runtime bridges TimerQueue → native timer primitives
}
```

`EmbassyRuntime` and `TokioRuntime` both implement `TimerService`, bridging
`TimerQueue` to their native timer primitives while keeping the blox-facing API
identical.

### Usage in a blox context

A blox that uses timers stores a `timer_ref` (an `ActorRef<TimerCommand, R>`) plus timer state in a behavior type injected at wiring time. The context is generic over `B` so the blox crate never references the concrete behavior:

```rust
#[derive(BloxCtx)]
pub struct PingCtx<
    R: BloxRuntime,
    B: HasCurrentTimer + CountsRounds,
> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
    pub peer_ref: ActorRef<PingPongMsg, R>,
    #[provides(HasSelfRef<R>)]
    pub self_ref: ActorRef<PingPongMsg, R>,
    #[provides(HasTimerRef<R>)]
    pub timer_ref: ActorRef<TimerCommand, R>,
    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
```

Timer state (the current `TimerId`) is held by `B` via the `HasCurrentTimer` trait. The blox spec wires trait-bounded action functions from `ping-pong-actions` into `on_entry`/`on_exit` slices:

```rust
// In ping-pong-actions — generic over HasTimerRef + HasCurrentTimer
pub fn schedule_resume<R, C>(ctx: &mut C, duration_ms: u64)
where
    R: BloxRuntime,
    C: HasSelfId + HasSelfRef<R> + HasTimerRef<R> + HasCurrentTimer,
{ ... }

pub fn cancel_current_timer<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasTimerRef<R> + HasCurrentTimer,
{ ... }
```

### Timer Pool in Embassy

`bloxide-embassy` provides `timer_task!` and `spawn_timer!` macros:

```rust
// In the application wiring module:
bloxide_embassy::timer_task!(timer_task);

fn setup(spawner: Spawner) {
    // spawn_timer! creates the channel internally and returns timer_ref
    let timer_ref = bloxide_embassy::spawn_timer!(spawner, timer_task, 8);
    // Pass timer_ref to blox contexts that need timers
}
```

## Actor ID Generation

Actor IDs are `ActorId = usize`. Actor IDs are allocated at compile time via
`next_actor_id!()`. `TIMER_ACTOR_ID` is a compile-time constant **hardcoded to
`0`** in `bloxide-timer`. The `next_actor_id!()` counter starts at `1`, so `0`
is permanently unoccupied by any actor channel allocated at compile time.
`TIMER_ACTOR_ID` is used as the `from` field in `Envelope`s delivered by the
timer service, making it distinguishable from real actor senders at inspection
time.

### Compile-time assignment via `channels!`

The `channels!` proc macro (in `bloxide-macros`) maintains a compile-time counter
— a `static AtomicUsize` inside the proc-macro crate. Each expansion of
`channels!` increments the counter and embeds the literal integer in the generated
code. No runtime counter is needed in production.

```rust
let ((ping_ref,), ping_mbox) = bloxide_embassy::channels! { PingPongMsg(16) };
let ((pong_ref,), pong_mbox) = bloxide_embassy::channels! { PingPongMsg(16) };
```

The first call embeds ID 1, the second embeds ID 2, and so on. Each `ActorRef`
stores its assigned ID as a `usize` field. The ID is used as the `from` field on
every outgoing `Envelope` so recipients know who sent a message.

### Non-channel ID allocation

For actors that don't go through `channels!` (e.g., supervisors that only receive
`ChildLifecycleEvent` via a hand-built channel), use `bloxide_embassy::next_actor_id!()`
which increments the same proc-macro counter:

```rust
let sup_id = bloxide_embassy::next_actor_id!();
```

### `TestRuntime` ID allocation

`TestRuntime` uses a runtime `AtomicUsize` counter since test channels are created
dynamically. `DynamicChannelCap::alloc_actor_id()` increments this counter and
returns the next ID.

## Relationship to HSM

The HSM engine interacts with capabilities only indirectly, through `Ctx`:

```
Event arrives
     │
     ▼
StateMachine::process_event
     │
     ├─▶ rule.actions(&mut ctx, &event)
     │       └─▶ set_timer(ctx, ...)              ← action function call in user code
     │
     ├─▶ state.on_entry(&mut ctx)
     │       └─▶ set_timer(ctx, ...)              ← action function call in user code
     │
     └─▶ state.on_exit(&mut ctx)
             └─▶ cancel_timer(ctx, ...)           ← action function call in user code
```

**Guards are pure.** `guard: fn(&Ctx, &ActionResults, &Event) -> Guard<S>` receives
`&Ctx` (shared reference) and `&ActionResults`, not `&mut Ctx`. This borrow-checks
the intent: a guard may inspect state and action results to decide which target
to transition to, but it must not fire side effects. Side effects belong in `actions`.

**The engine never calls runtime methods directly.** `StateMachine` is generic
over `S: MachineSpec` and knows nothing about `BloxRuntime`, `TimerService`, or any
other trait.

## TestRuntime Contract

Every Tier 2 trait **must** be implementable by `TestRuntime` so that blox
logic can be unit-tested without an executor. `TestRuntime` lives in
`bloxide-core` behind the `std` feature and provides:

- `BloxRuntime` — unbounded in-memory queues; `try_send` never returns an error.
- `DynamicChannelCap` — creates `(ActorRef, TestReceiver)` pairs on demand.

Timer testing is not built into `TestRuntime` itself, but `bloxide-timer`
provides a reusable std-only helper: `bloxide_timer::test_utils::VirtualClock`.
It owns the timer command receiver, drains pending `TimerCommand`s into a
`TimerQueue`, and fires ready callbacks when time advances. This keeps timer
simulation deterministic without requiring any executor or creating a circular
dependency from `bloxide-core` back to `bloxide-timer`.

### Typical test pattern

```rust
use bloxide_core::{DynamicChannelCap, TestRuntime};
use bloxide_timer::{test_utils::VirtualClock, TimerCommand};

#[test]
fn paused_state_resumes_after_timeout() {
    let ping_id = <TestRuntime as DynamicChannelCap>::alloc_actor_id();
    let (self_ref, mut to_ping_rx) =
        <TestRuntime as DynamicChannelCap>::channel::<PingPongMsg>(ping_id, 16);
    let pong_id = <TestRuntime as DynamicChannelCap>::alloc_actor_id();
    let (pong_ref, _) =
        <TestRuntime as DynamicChannelCap>::channel::<PingPongMsg>(pong_id, 16);
    let timer_id = <TestRuntime as DynamicChannelCap>::alloc_actor_id();
    let (timer_ref, mut timer_rx) =
        <TestRuntime as DynamicChannelCap>::channel::<TimerCommand>(timer_id, 16);

    let ctx = PingCtx::new(ping_id, pong_ref, self_ref, timer_ref, TestBehavior::default());
    let mut machine = StateMachine::new(ctx);

    machine.start();
    // ... drive rounds until Paused ...

    // Manually advance the virtual clock; ready callbacks enqueue Resume.
    let mut clock = VirtualClock::new(timer_rx);
    clock.advance(PAUSE_DURATION_MS);

    // Resume should now be in the mailbox
    let msgs = to_ping_rx.drain_payloads();
    assert_eq!(msgs.len(), 1);
}
```

The test harness approach is synchronous and deterministic. Tests never need
`sleep`, `tokio::time::pause`, or a real executor.

## `no_std` Compatibility

All traits and the mechanisms described in this document are compatible
with `no_std`:

| Concern | Solution |
|---|---|
| Actor ID generation (production) | Proc-macro counter assigns literal IDs at compile time via `channels!` and `next_actor_id!`; no runtime counter |
| Actor ID generation (test) | `TestRuntime` uses a runtime `AtomicUsize` via `DynamicChannelCap::alloc_actor_id()` |
| Timer ID generation | `TimerId` assigned by `set_timer()` in `bloxide-timer`; uses core atomics on pointer-atomic targets and a `critical-section`-protected counter otherwise |
| `TestRuntime` | Uses `std` (enabled by the `std` feature); only used in host tests |
| Action crates | `#![no_std]`; call only trait methods; no OS imports |
| Core traits | Defined in `bloxide-core` which is `#![no_std]` |
| `critical-section` | Used by `bloxide-timer` as the fallback for targets without pointer-sized atomics; embedded apps must provide an implementation via their HAL/runtime stack |

**`bloxide-core` invariant**: zero OS, Tokio, or Embassy imports in any file.
The only permitted external dependency is `futures-core` for the `Stream` bound on
`BloxRuntime::Stream`.

For embedded targets such as ESP32-C3 (`riscv32imc-unknown-none-elf`), this keeps
`bloxide-timer` buildable without hardware atomics while avoiding target-specific
`portable-atomic` cfg requirements in the framework itself.
