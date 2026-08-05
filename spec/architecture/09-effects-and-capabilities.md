# Effects and Capabilities

> **When would I use this?** Use this document when implementing timer
> patterns, understanding how capabilities flow through the action layer, or
> working with `TestRuntime` and a virtual clock for deterministic testing.
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

Blox crates are generic over a single Tier 1 trait: `R: BloxRuntime`. All additional capabilities (timers, supervision) are exposed through **standard library crates** that define action functions and messages — never as additional runtime bounds on the blox.

```
┌─────────────────────────────────────────────────────────────┐
│                          Blox Crate                         │
│  on_entry / on_exit / actions  ──calls──▶  action functions │
│       (pure logic + guard)       send_ping(...)             │
│                                  cancel_timer(...)          │
└─────────────────────────────────────────────────────────────┘
                            │
          Tier 1: R: BloxRuntime only
                            │
┌─────────────────────────────────────────────────────────────┐
│               Standard Library Crates (Layer 2)             │
│  bloxide-timer: TimerCommand, TimerQueue,                  │
│                 set_timer(), cancel_timer(),                │
│                 cancel_timer_by_id()                        │
│  bloxide-child-management: ChildGroup, ChildPolicy, ...     │
└─────────────────────────────────────────────────────────────┘
                            │
          Tier 2: TimerService (run loop: run() + RunConfig, in bloxide-core)
                            │
┌─────────────────────────────────────────────────────────────┐
│                       Runtime Crate                          │
│       EmbassyRuntime   or   TestRuntime   or   TokioRuntime │
│       impl BloxRuntime + TimerService                        │
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
```

Action functions for blox code take concrete params:

```rust
/// Schedule `event` to be delivered to `target` after `after_ms` milliseconds.
/// Returns `Some(TimerId)` for cancellation, or `None` when the timer
/// channel is full (the command could not be queued).
pub fn set_timer<R, M>(
    self_id: ActorId,
    timer_ref: &ActorRef<TimerCommand, R>,
    after_ms: u64,
    target: &ActorRef<M, R>,
    event: M,
) -> Option<TimerId>
where
    R: BloxRuntime,
    M: Send + 'static;

/// Cancel a pending timer.
pub fn cancel_timer<R>(
    self_id: ActorId,
    timer_ref: &ActorRef<TimerCommand, R>,
    id: TimerId,
) -> ActionResult
where
    R: BloxRuntime;
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

A blox that uses timers stores a `timer_ref` (an `ActorRef<TimerCommand, R>`) plus timer state as plain fields. The context is a plain struct — no `B` generic, no accessor traits:

```rust
pub struct PingCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub peer_ref: ActorRef<PingPongMsg, R>,
    pub self_ref: ActorRef<PingPongMsg, R>,
    pub timer_ref: ActorRef<TimerCommand, R>,
    pub current_timer: Option<TimerId>,
    pub round: u32,
}
```

Timer state (the current `TimerId`) is held as a plain field. The blox spec wires action functions from context crates into `on_entry`/`on_exit` slices:

```rust
// In blox-ctx-ping-pong — takes concrete params; the pause duration is
// derived from the round (2000 + round × 500 ms). Returns ActionResult.
pub fn schedule_resume<R: BloxRuntime>(
    self_id: ActorId,
    self_ref: &ActorRef<PingPongMsg, R>,
    timer_ref: &ActorRef<TimerCommand, R>,
    round: u32,
    current_timer: &mut Option<TimerId>,
) -> ActionResult { ... }

// In bloxide-timer (module `actions`, also re-exported at the crate root
// and in the prelude). Takes the current-timer field by &mut and clears it.
pub fn cancel_timer_by_id<R: BloxRuntime>(
    self_id: ActorId,
    timer_ref: &ActorRef<TimerCommand, R>,
    current_timer: &mut Option<TimerId>,
) -> ActionResult { ... }
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
returns the next ID. The counter starts at `DYNAMIC_ACTOR_ID_BASE` (256,
in `bloxide-core::capability`) so runtime-allocated IDs can never collide with
the compile-time IDs handed out by `channels!` / `next_actor_id!`. The static
space is `1..=255` — a hard limit of 255 statically wired actors per system;
the wiring macros bake a `const _: () = assert!(id < DYNAMIC_ACTOR_ID_BASE)`
guard into their expansion, so exceeding the limit is a compile error.

## Relationship to HSM

The HSM engine interacts with capabilities only indirectly, through `Ctx`:

```
Event arrives
     │
     ▼
StateMachine::process_event
     │
     ├─▶ rule.actions(&mut ctx, &event)
     │       └─▶ send_ping(...)               ← action function call in user code
     │
     ├─▶ state.on_entry(&mut ctx)
     │       └─▶ set_timer(...)              ← action function call in user code
     │
     └─▶ state.on_exit(&mut ctx)
             └─▶ cancel_timer(...)           ← action function call in user code
```

**Guards are pure.** `guard: fn(&Ctx, &ActionResults, &Event) -> Decision<S>` receives
`&Ctx` (shared reference) and `&ActionResults`, not `&mut Ctx`. This borrow-checks
the intent: a guard may inspect state and action results to decide which target
to transition to, but it must not fire side effects. Side effects belong in `actions`.

**The engine never calls runtime methods directly.** `StateMachine` is generic
over `S: MachineSpec` and knows nothing about `BloxRuntime`, `TimerService`, or any
other trait.

## TestRuntime Contract

Every Tier 2 trait **must** be implementable by `TestRuntime` so that blox
logic can be unit-tested without an executor. `TestRuntime` lives in
`runtimes/bloxide-test-runtime` (its own crate — not behind a `bloxide-core`
feature) and provides:

- `BloxRuntime` — in-memory queues. Capacity **is** enforced for `try_send`
  (the backpressure path action functions use); `send_via` is unbounded and
  never fails. Receivers model all-streams-close semantics (issue #134):
  dropping the last sender wakes the receiver, which drains queued envelopes
  and then returns `Poll::Ready(None)`.
- `DynamicChannelCap` — creates `(ActorRef, TestReceiver)` pairs on demand;
  `alloc_actor_id()` hands out IDs starting at `DYNAMIC_ACTOR_ID_BASE`
  (256) so they can never collide with compile-time IDs.
- `SpawnCap` — dynamic spawning in tests; handles are `usize` spawn ids
  (`KillHandle = usize`). `kill` records the id in a thread-local log rather
  than destroying a task (TestRuntime runs no real tasks); tests assert via
  `drain_killed()` / `kill_count()`. The `Kill` adapter in `bloxide-spawn`
  reports `CAN_KILL = true` for this runtime.

Timer testing is not built into `TestRuntime` itself. The reference pattern is
`bloxide-timer`'s `VirtualClock` (`crates/bloxide-timer/src/test_utils.rs`):
it owns the timer command receiver, drains pending `TimerCommand`s into a
`TimerQueue`, and fires ready callbacks when time advances. `VirtualClock` is
`#[cfg(test)]`-gated and crate-internal — other crates cannot import it, so
blox tests replicate the same few lines inline (shown below). This keeps timer
simulation deterministic without requiring any executor or creating a circular
dependency from `bloxide-core` back to `bloxide-timer`.

### Typical test pattern

```rust
use bloxide_core::DynamicChannelCap;
use bloxide_test_runtime::TestRuntime;
use bloxide_timer::{TimerCommand, TimerQueue};

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

    let ctx = PingCtx::new(ping_id, pong_ref, self_ref, timer_ref);
    let mut machine = StateMachine::new(ctx);

    machine.dispatch(PingEvent::Lifecycle(LifecycleCommand::Start));
    // ... drive rounds until Paused ...

    // Drive a virtual clock (mirrors bloxide-timer's crate-internal
    // VirtualClock): drain pending TimerCommands into a TimerQueue, advance
    // a manual clock past the scheduled resume (schedule_resume computes
    // 2000 + round × 500 ms from the round), and fire ready callbacks.
    let mut queue = TimerQueue::new();
    for cmd in timer_rx.drain_payloads() {
        queue.handle_command(cmd, 0);
    }
    let now_ms = 2000 + PAUSE_AT_ROUND as u64 * 500;
    for deliver in queue.drain_expired(now_ms) {
        deliver();
    }

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
| Actor ID generation (test) | `TestRuntime` uses a runtime `AtomicUsize` via `DynamicChannelCap::alloc_actor_id()`, starting at `DYNAMIC_ACTOR_ID_BASE` |
| Timer ID generation | `TimerId` assigned by `set_timer()` in `bloxide-timer`; uses core atomics on pointer-atomic targets and a `critical-section`-protected counter otherwise |
| `TestRuntime` | Own crate (`runtimes/bloxide-test-runtime`), uses `std`; only used in host tests |
| Action crates | `#![no_std]`; call only concrete params; no OS imports |
| Core traits | Defined in `bloxide-core` which is `#![no_std]` |
| `critical-section` | Used by `bloxide-timer` as the fallback for targets without pointer-sized atomics; embedded apps must provide an implementation via their HAL/runtime stack |

**`bloxide-core` invariant**: zero OS, Tokio, or Embassy imports in any file.
The only permitted external dependency is `futures-core` for the `Stream` bound on
`BloxRuntime::Stream`.

For embedded targets such as ESP32-C3 (`riscv32imc-unknown-none-elf`), this keeps
`bloxide-timer` buildable without hardware atomics while avoiding target-specific
`portable-atomic` cfg requirements in the framework itself.
