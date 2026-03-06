# Building with Bloxide — Reference

Deep-dive companion to SKILL.md. Read this when you need detailed syntax or patterns.

## `#[derive(BloxCtx)]` Annotation Reference

| Annotation | Generated code | Notes |
|---|---|---|
| `#[self_id]` | `impl HasSelfId for Ctx { fn self_id() -> ActorId }` | Exactly one field. Constructor arg. |
| `#[provides(Trait<R>)]` | `impl Trait<R> for Ctx { fn field_name() -> &FieldType }` | Field name must match trait method name. Constructor arg. |
| `#[delegates(TraitA, TraitB)]` | Forwarding impls via `__delegate_TraitName!()` macros | Each listed trait must be `#[delegatable]`. Constructor arg. |
| _(none)_ | Field initialized via `Default::default()` | Not a constructor arg. |

The generated `fn new(...)` takes constructor args in field declaration order: `#[self_id]` first, then `#[provides]` fields, then `#[delegates]` fields.

## `transitions!` Macro Syntax

The `transitions!` macro builds a `&'static [StateRule<S>]` slice. Every rule has this shape:

```
PATTERN => BODY,
```

### Event patterns

**Full event pattern** — matches the outer event enum variant:

```rust
MyEvent::Msg(Envelope(_, MyProtocolMsg::Request(req))) => { ... }
```

**Message shorthand** — the macro detects types ending in `Msg` and generates the envelope unwrap:

```rust
MyProtocolMsg::Request(req) => { ... }
```

### Body forms

```rust
// Pure Transition
MyMsg::Foo(_) => { transition MyState::Bar }

// Sink (absorb, prevent bubbling)
MyMsg::Foo(_) => stay,

// Reset (self-terminate via engine enter_init path)
MyMsg::Foo(_) => reset,

// Actions + stay
MyMsg::Foo(foo) => {
    actions [Self::action_a, Self::action_b]
    stay
}

// Actions + unconditional transition
MyMsg::Foo(foo) => {
    actions [Self::action_a]
    transition MyState::Bar
}

// Actions + conditional guard
MyMsg::Foo(foo) => {
    actions [Self::action_a, Self::action_b]
    guard(ctx, results) {
        results.any_failed()  => MyState::Error,
        ctx.count() >= MAX    => MyState::Done,
        _                     => MyState::Active,
    }
}

// Guard only (no actions)
MyMsg::Tick(_) => {
    guard(ctx, _results) {
        ctx.deadline_elapsed() => MyState::Timeout,
        _                      => stay,
    }
}

// Actions + conditional reset
MyMsg::ChildDone(_) => {
    actions [Self::record_child_done]
    guard(ctx, _results) {
        ctx.all_done() => reset,
        _              => stay,
    }
}
```

Inside `guard(ctx, results) { ... }`:
- `ctx` is `&Ctx` (read-only)
- `results` is `&ActionResults`
- `stay` keeps current state
- `reset` triggers full exit chain then `on_init_entry`
- State variants trigger LCA transition

Inside `actions [fn1, fn2]`:
- Each function is `fn(&mut Ctx, &Event) -> ActionResult`
- Executed in order; results collected into `ActionResults`
- Use `ActionResult::from(try_send_result)` for send operations

## `on_entry`/`on_exit` Action Slices

`StateFns` uses slices for composable entry/exit actions:

```rust
const ACTIVE_FNS: StateFns<Self> = StateFns {
    on_entry: &[increment_round, send_initial_ping],
    on_exit:  &[cancel_current_timer],
    transitions: transitions![ ... ],
};
```

Each function is `fn(&mut Ctx)` (infallible, no event access). Multiple actions execute in slice order. Entry actions from action crates compose with blox-local helpers.

## `#[blox_event]` and Mailboxes

The `#[blox_event]` attribute generates `EventTag` impl and stream-to-event conversion. Each enum variant maps to one mailbox stream:

```rust
#[blox_event]
#[derive(Debug)]
pub enum MyEvent {
    Msg(Envelope<MyProtocolMsg>),      // from Mailbox stream 0
    Timer(Envelope<TimerCommand>),     // from Mailbox stream 1
}
```

The `Mailboxes` associated type in `MachineSpec` is a tuple of streams matching the variant order:

```rust
type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<MyProtocolMsg>, Rt::Stream<TimerCommand>);
```

Use `ev.msg_payload()` in action functions to extract the message from the envelope.

## Timer Patterns

Timers use the `bloxide-timer` crate. The blox needs:
- `HasTimerRef<R>` accessor trait (provided by `#[provides]`)
- `HasCurrentTimer` behavior trait (for storing the pending timer ID)

### Setting a timer

```rust
use bloxide_timer::{set_timer, HasTimerRef, TimerId};

pub fn schedule_resume<R, C>(ctx: &mut C, duration_ms: u64)
where
    R: BloxRuntime,
    C: HasSelfRef<R> + HasTimerRef<R> + HasSelfId + HasCurrentTimer,
{
    let id = set_timer::<R, C, PingPongMsg>(
        ctx, duration_ms, ctx.self_ref(), PingPongMsg::Resume(Resume),
    );
    ctx.set_current_timer(Some(id));
}
```

`set_timer` sends a `TimerCommand::Set` to the timer service via `ctx.timer_ref()`. After `duration_ms`, the timer service sends the payload message to `self_ref`.

### Canceling a timer

```rust
pub fn cancel_current_timer<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasTimerRef<R> + HasCurrentTimer,
{
    if let Some(id) = ctx.current_timer() {
        cancel_timer::<R, C>(ctx, id);
        ctx.set_current_timer(None);
    }
}
```

### Timer context setup

```rust
#[derive(BloxCtx)]
pub struct MyCtx<R: BloxRuntime, B: HasCurrentTimer + CountsRounds> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasSelfRef<R>)]
    pub self_ref: ActorRef<MyProtocolMsg, R>,
    #[provides(HasTimerRef<R>)]
    pub timer_ref: ActorRef<TimerCommand, R>,
    #[delegates(HasCurrentTimer, CountsRounds)]
    pub behavior: B,
}
```

## Supervision Patterns

Supervision uses `bloxide-supervisor`. The supervisor is a reusable blox — you don't write one, you configure it.

### Wiring a supervisor

```rust
let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);

runtime::spawn_child!(group, my_task(machine, mbox, id),
    ChildPolicy::Restart { max: 3 });

runtime::spawn_child!(group, other_task(machine, mbox, id),
    ChildPolicy::Stop);

let (children, sup_notify_rx) = group.finish();
let sup_ctx = SupervisorCtx::new(sup_id, children);
let mut sup_machine = StateMachine::<SupervisorSpec<R>>::new(sup_ctx);
sup_machine.start();
```

### Child policies

| Policy | Behavior |
|---|---|
| `ChildPolicy::Restart { max }` | Restart child up to `max` times on failure/done |
| `ChildPolicy::Stop` | Permanently stop child on failure/done |

### Group shutdown strategies

| Strategy | Behavior |
|---|---|
| `GroupShutdown::WhenAnyDone` | Begin shutdown when any child reaches terminal state |
| `GroupShutdown::WhenAllDone` | Begin shutdown when all children reach terminal state |

### How supervision works from the blox perspective

Bloxes are unaware of supervision. The runtime manages lifecycle:
- `machine.start()` enters `initial_state()` from Init
- `machine.reset()` exits all states and re-enters Init (for restart)
- `is_terminal(state)` signals Done to the supervisor
- `is_error(state)` signals Failed (takes precedence over `is_terminal`)

No lifecycle message types, no supervisor refs in blox contexts.

## Dynamic Actor Spawning

For runtimes that support it (Tokio), actors can spawn new actors at runtime using `bloxide-spawn`.

### Factory injection

The binary provides a spawn factory function to the parent actor's context:

```rust
pub type WorkerSpawnFn<R> = Box<dyn Fn(ActorId, /* other args */) -> JoinHandle<()> + Send>;
```

The parent stores the factory and calls it to spawn children dynamically.

### Peer introduction

After spawning a new actor, use `introduce_peers` to exchange `ActorRef`s between existing actors and the new one. This uses `PeerCtrl` messages sent through a control channel.

## Topology Patterns

### Flat FSM

All states are leaves. Simplest topology.

```
[VirtualRoot]
├── Ready  (leaf)
├── Active (leaf)
└── Done   (leaf)
```

### Composite with shared handler

Parent composite state handles events common to all children (typically Sink rules).

```
[VirtualRoot]
└── Operating  (composite)
    ├── Active (leaf)
    └── Paused (leaf)
```

### Pause/Resume

Active leaf + Paused leaf under a composite parent. Paused sets a timer on entry; Resume transitions back to Active.

### Terminal with notification

Override `is_terminal` on the spec. The runtime auto-notifies the supervisor. The Done state itself needs no special logic.

```rust
fn is_terminal(state: &MyState) -> bool {
    matches!(state, MyState::Done)
}
```

### Error state

Override `is_error`. The runtime emits `ChildLifecycleEvent::Failed`. `is_error` takes precedence over `is_terminal`.

```rust
fn is_error(state: &MyState) -> bool {
    matches!(state, MyState::Error)
}
```

## Complete Worked Example: Pong Blox

Pong is the simplest possible blox. It receives `Ping` messages and replies with `Pong`.

### Messages (shared crate)

```rust
#[derive(Debug, Clone, Copy)]
pub struct Ping { pub round: u32 }

#[derive(Debug, Clone, Copy)]
pub struct Pong { pub round: u32 }

#[derive(Debug, Clone)]
pub enum PingPongMsg {
    Ping(Ping),
    Pong(Pong),
    Resume(Resume),
}
```

### Actions (actions crate)

```rust
pub trait HasPeerRef<R: BloxRuntime> {
    fn peer_ref(&self) -> &ActorRef<PingPongMsg, R>;
}

pub fn send_pong<R, C>(ctx: &mut C, ping: &Ping) -> ActionResult
where
    R: BloxRuntime, C: HasSelfId + HasPeerRef<R>,
{
    ActionResult::from(ctx.peer_ref().try_send(
        ctx.self_id(), PingPongMsg::Pong(Pong { round: ping.round }),
    ))
}
```

### Context

```rust
#[derive(BloxCtx)]
pub struct PongCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPeerRef<R>)]
    pub peer_ref: ActorRef<PingPongMsg, R>,
}
```

### Events

```rust
#[blox_event]
#[derive(Debug)]
pub enum PongEvent {
    Msg(Envelope<PingPongMsg>),
}
```

### State topology

```rust
#[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
#[handler_fns(READY_FNS, ERROR_FNS)]
pub enum PongState {
    Ready,
    Error,
}
```

### MachineSpec

```rust
impl<R: BloxRuntime> MachineSpec for PongSpec<R> {
    type State = PongState;
    type Event = PongEvent;
    type Ctx = PongCtx<R>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<PingPongMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = pong_state_handler_table!(Self);

    fn initial_state() -> PongState { PongState::Ready }

    fn is_error(state: &PongState) -> bool {
        matches!(state, PongState::Error)
    }

    fn on_init_entry(ctx: &mut PongCtx<R>) {
        bloxide_log::blox_log_info!(ctx.self_id(), "reset");
    }
}
```

### Handlers

```rust
const READY_FNS: StateFns<Self> = StateFns {
    on_entry: &[],
    on_exit: &[],
    transitions: transitions![
        PingPongMsg::Ping(_ping) => {
            actions [Self::reply_pong_action]
            guard(_ctx, results) {
                results.any_failed() => PongState::Error,
                _                    => stay,
            }
        },
    ],
};

const ERROR_FNS: StateFns<Self> = StateFns {
    on_entry: &[Self::log_error],
    on_exit: &[],
    transitions: &[],
};
```

### Wiring (binary)

```rust
let ((pong_ref,), pong_mbox) = runtime::channels! { PingPongMsg(16) };
let pong_ctx = PongCtx::new(pong_id, ping_ref);
let pong_machine = StateMachine::new(pong_ctx);
// Spawn as supervised child or standalone
```
