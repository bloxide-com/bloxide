# Building with Bloxide

This guide teaches you how to build actors ("bloxes") using the Bloxide framework. It is self-contained and portable — copy it into any project that depends on bloxide crates.

## What Bloxide Is

Bloxide is a `no_std` hierarchical state machine (HSM) + actor messaging framework for Rust. Domain actors ("bloxes") implement the `MachineSpec` trait to define state topologies, event handlers, and context. Blox code is generic over `R: BloxRuntime` so the same state machine runs on Embassy (embedded) and Tokio (std) without modification. A separate runtime crate wires channels, spawns tasks, and drives the machine.

## Five-Layer Architecture

Every bloxide application is built from five layers. Each lives in its own crate with strict dependency rules.

```
Layer 1 — Messages     Pure data structs. No logic, no ActorRef.
Layer 2 — Actions       Accessor traits, behavior traits, generic action functions. Interface only.
Layer 3 — Impl          Concrete types implementing behavior traits. Owned by the binary author.
Layer 4 — Blox          Declarative wiring: state topology, context struct, MachineSpec.
Layer 5 — Binary        Channels, context construction, task spawning.
```

### Layer 1 — Messages crate

A `*-messages` crate contains shared message enums used by two or more bloxes. Messages are plain data with named struct variants.

```rust
#![no_std]

#[derive(Debug, Clone, Copy)]
pub struct Ping { pub round: u32 }

#[derive(Debug, Clone, Copy)]
pub struct Pong { pub round: u32 }

#[derive(Debug, Clone, Copy)]
pub struct Resume;

#[derive(Debug, Clone)]
pub enum PingPongMsg {
    Ping(Ping),
    Pong(Pong),
    Resume(Resume),
}
```

### Layer 2 — Actions crate

An actions crate (`*-actions`) defines traits and trait-bounded generic functions. Zero concrete implementations.

**Accessor traits** expose context fields generically:

```rust
pub trait HasPeerRef<R: BloxRuntime> {
    fn peer_ref(&self) -> &ActorRef<PingPongMsg, R>;
}
```

**Behavior traits** define domain capabilities:

```rust
#[delegatable]
pub trait CountsRounds {
    type Round: Copy + PartialEq + PartialOrd + core::ops::Add<Output = Self::Round>
              + From<u8> + core::fmt::Display;
    fn round(&self) -> Self::Round;
    fn set_round(&mut self, round: Self::Round);
}
```

**Generic action functions** use trait bounds, not concrete types:

```rust
pub fn increment_round<C: CountsRounds>(ctx: &mut C) {
    let one = C::Round::from(1);
    ctx.set_round(ctx.round() + one);
}

pub fn send_ping<R, C>(ctx: &mut C) -> ActionResult
where
    R: BloxRuntime,
    C: HasSelfId + HasPeerRef<R> + CountsRounds,
    C::Round: Into<u32>,
{
    ActionResult::from(ctx.peer_ref().try_send(
        ctx.self_id(),
        PingPongMsg::Ping(Ping { round: ctx.round().into() }),
    ))
}
```

### Layer 3 — Impl crate

Provides concrete types that implement behavior traits. Owned by the binary author, not the blox. Only needed when a blox has mutable behavior state beyond its accessor fields.

Example: `PingBehavior` implements `CountsRounds`, `HasCurrentTimer`, etc. A different impl crate could supply hardware-backed implementations of the same traits while reusing the same blox crate.

Bloxes with no mutable behavior state (like Pong) skip this layer entirely.

### Layer 4 — Blox crate

A blox crate is primarily declarative. It defines:

1. **State topology** via `#[derive(StateTopology)]`
2. **Context struct** via `#[derive(BloxCtx)]` with field annotations
3. **Event enum** via `#[blox_event]`
4. **`MachineSpec`** with `StateFns` tables and `transitions!` macro

### Layer 5 — Binary

The binary creates channels, constructs contexts, and spawns tasks:

```rust
let ((ping_ref,), ping_mbox) = runtime::channels! { PingPongMsg(16) };
let ((pong_ref,), pong_mbox) = runtime::channels! { PingPongMsg(16) };
let ping_machine = StateMachine::new(PingCtx::new(ping_id, pong_ref, self_ref, timer_ref, behavior));
let pong_machine = StateMachine::new(PongCtx::new(pong_id, ping_ref));
```

## Spec-Driven Development Workflow

Follow this workflow for every new blox:

1. **Spec** — Write `spec/bloxes/<name>.md` describing states, events, context, and acceptance criteria. Use a stateDiagram-v2 for the state hierarchy.
2. **Review** — Verify the state diagram, transition tables, and acceptance criteria are consistent.
3. **Test** — Write unit tests using `TestRuntime`, one test per acceptance criterion. Tests use `StateMachine::new`, `machine.start()`, and `machine.dispatch()`.
4. **Implement** — Write the Rust code (messages, actions, blox crate) to make tests pass.
5. **Sync** — If implementation reveals spec errors, update the spec. The spec is always ahead of or equal to the code.

## Creating a New Blox Step-by-Step

### Step 1: Define the message enum

Create a `*-messages` crate if two or more bloxes share a protocol. Use named struct variants:

```rust
pub enum MyProtocolMsg {
    Request(Request),    // Request { payload: Vec<u8> }
    Response(Response),  // Response { status: u16, data: Vec<u8> }
}
```

If the blox only receives messages defined elsewhere, skip this step.

### Step 2: Define traits in an actions crate

Create a `*-actions` crate with:
- Accessor traits for each `ActorRef` the blox context will hold
- Behavior traits for domain state the blox needs to expose
- Generic action functions that implement side effects

Mark behavior traits with `#[delegatable]` if they will be delegated to a behavior field.

### Step 3: Define the event enum

Use `#[blox_event]` to generate the event type and `EventTag` impl:

```rust
#[blox_event]
#[derive(Debug)]
pub enum MyEvent {
    Msg(Envelope<MyProtocolMsg>),
}
```

Each variant wraps a different mailbox stream. Most bloxes have a single `Msg` variant.

### Step 4: Define the context struct

Use `#[derive(BloxCtx)]` with field annotations:

```rust
#[derive(BloxCtx)]
pub struct MyCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
    pub peer_ref: ActorRef<MyProtocolMsg, R>,
}
```

| Annotation | Effect |
|---|---|
| `#[self_id]` | Implements `HasSelfId`; field initialized from constructor arg |
| `#[provides(Trait<R>)]` | Implements the accessor trait; field name = trait method name |
| `#[delegates(TraitA, TraitB)]` | Forwards trait calls to a generic behavior field |
| _(none)_ | Initialized via `Default::default()` in the generated constructor |

For bloxes with behavior state, add a generic parameter:

```rust
#[derive(BloxCtx)]
pub struct MyCtx<R: BloxRuntime, B: CountsRounds + SomeOtherTrait> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
    pub peer_ref: ActorRef<MyProtocolMsg, R>,
    #[delegates(CountsRounds, SomeOtherTrait)]
    pub behavior: B,
}
```

### Step 5: Define state topology and MachineSpec

```rust
#[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
#[handler_fns(READY_FNS, ERROR_FNS)]
pub enum MyState {
    Ready,
    Error,
}
```

The `#[handler_fns(...)]` attribute maps each variant to a `StateFns` constant. For composite states, implement `parent()` via `StateTopology` (the derive macro handles flat topologies automatically).

Implement `MachineSpec`:

```rust
impl<R: BloxRuntime> MachineSpec for MySpec<R> {
    type State = MyState;
    type Event = MyEvent;
    type Ctx = MyCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<MyProtocolMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = my_state_handler_table!(Self);

    fn initial_state() -> MyState { MyState::Ready }

    fn on_init_entry(ctx: &mut MyCtx<R>) {
        // Reset domain state here (counters, timers, etc.)
    }

    fn is_terminal(state: &MyState) -> bool {
        matches!(state, MyState::Done)
    }

    fn is_error(state: &MyState) -> bool {
        matches!(state, MyState::Error)
    }
}
```

### Step 6: Define handlers with `transitions!`

```rust
const READY_FNS: StateFns<Self> = StateFns {
    on_entry: &[],
    on_exit: &[],
    transitions: transitions![
        // Action-Then-Guard: run side effects, then decide
        MyProtocolMsg::Request(req) => {
            actions [Self::handle_request_action]
            guard(ctx, results) {
                results.any_failed() => MyState::Error,
                _                    => stay,
            }
        },
    ],
};
```

Handler patterns (use these by name in specs):
- **Pure Transition** — `MyMsg::Foo(_) => { transition MyState::Bar }`
- **Sink** — `MyMsg::Foo(_) => stay,` (absorbs event, prevents bubbling)
- **Action-Then-Stay** — actions with `stay`
- **Action-Then-Guard** — actions followed by conditional guard
- **Pure Guard** — guard only, no actions
- **Bubble** — no rule (event automatically bubbles to parent)

### Step 7: Wire in the binary

Create channels, construct contexts with `::new(...)`, build `StateMachine` instances, and spawn tasks using runtime macros.

## Spec Sync

After implementing a blox, if the code diverges from the spec:

1. Update the state hierarchy diagram in the spec
2. Update the states and events tables
3. Update context fields and annotations
4. Re-verify acceptance criteria match the implementation
5. Add any new acceptance criteria discovered during implementation

The spec must always be ahead of or equal to the code.

## Key Invariants

These must never be violated. Breaking them causes subtle bugs or compilation failures.

1. **Blox crates are runtime-agnostic** — generic over `R: BloxRuntime`. Never import a runtime crate from a blox.
2. **No runtime types in messages** — message enums contain plain data only. No `ActorRef`, no raw senders.
3. **Shared messages in dedicated crates** — message types used by two or more bloxes live in a `*-messages` crate.
4. **Only leaf states as transition targets** — the engine `debug_assert`s this. Violating it in release is undefined behavior.
5. **`on_entry`/`on_exit` are infallible** — `fn(&mut Ctx)` with no `Result`. Fallible work goes in transition `actions`.
6. **Actions before guards** — side effects go in `actions: fn(&mut Ctx, &Event) -> ActionResult`. Guards are pure: `fn(&Ctx, &ActionResults, &Event) -> Guard`. The borrow checker enforces this.
7. **Bubbling is implicit** — states with no matching rule bubble to the parent. Never add a catch-all that manually returns `Parent`.
8. **Blox crates never import impl crates** — concrete types are only referenced by the binary.
9. **Action crates are interface-only** — traits and trait-bounded generic functions. Zero concrete logic.
10. **Named struct variants in message enums** — `Ping(Ping { round })` not `Ping(u32)`.
11. **Logging via `bloxide-log` macros** — `blox_log_info!`, `blox_log_debug!`, etc. Compile-time feature flag, not a runtime trait.
12. **Actors never handle lifecycle events** — the runtime manages Start, Terminate, Stop via `machine.start()` and `machine.reset()`. No lifecycle message types or supervisor refs in blox crates.

## Further Reading

For deeper coverage of specific topics, see `reference.md` in this directory.
