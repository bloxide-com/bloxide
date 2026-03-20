# Building with Bloxide — Reference

Quick reference for macro syntax, common patterns, and generated code.

## Macros Quick Reference

### `blox_messages!` — Message Enum

```rust
blox_messages! {
    pub enum PingPongMsg {
        Ping { round: u32 },
        Pong { round: u32 },
        Resume {},
    }
}
```

Generates:
- `pub struct Ping { pub round: u32 }`
- `pub struct Pong { pub round: u32 }`
- `pub struct Resume {}`
- `pub enum PingPongMsg { Ping(Ping), Pong(Pong), Resume(Resume) }`

### `event!` — Event Enum

```rust
// Single mailbox
event!(Ping { Msg: PingPongMsg });

// Multiple mailboxes (index 0 polled first)
event!(Worker<R> { 
    Ctrl: WorkerCtrl<R>, 
    Msg: WorkerMsg 
});
```

Generates:
- `pub enum PingEvent { Msg(Envelope<PingPongMsg>) }`
- `EventTag` impl with `MSG_TAG` constant
- `msg_payload(&self) -> Option<&PingPongMsg>` helper
- `From<Envelope<M>>` for stream conversion

### `#[derive(BloxCtx)]` — Context Struct

**Field conventions (auto-detected):**

| Field | Generated |
|-------|-----------|
| `self_id: ActorId` | `impl HasSelfId` |
| `peer_ref: ActorRef<M, R>` | `impl HasPeerRef<R>` |
| `timer_ref: ActorRef<TimerCommand, R>` | `impl HasTimerRef<R>` |
| `pool_ref: ActorRef<PoolMsg, R>` | `impl HasPoolRef<R>` |
| `worker_factory: fn(...) -> ...` | Constructor parameter (no trait) |

**Required annotation:**

```rust
#[delegates(TraitA, TraitB)]
pub behavior: B,
```

Generates forwarding impls. Import `__delegate_TraitA` from the action crate.

**Generated constructor signature:**

```rust
// For fields: self_id, peer_ref, timer_ref, worker_factory, behavior
fn new(
    self_id: ActorId,
    peer_ref: ActorRef<M, R>,
    timer_ref: ActorRef<TimerCommand, R>,
    worker_factory: WorkerSpawnFn<R>,
    behavior: B,
) -> Self
```

### `#[derive(StateTopology)]` — State Enum

```rust
#[derive(StateTopology, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
#[handler_fns(ACTIVE_FNS, PAUSED_FNS, DONE_FNS)]
pub enum PingState {
    #[composite]
    Operating,
    #[parent(Operating)]
    Active,
    #[parent(Operating)]
    Paused,
    Done,
    Error,
}
```

Generates:
- `parent(&self) -> Option<Self>`
- `is_leaf(&self) -> bool`
- `as_index(&self) -> usize`
- `STATE_COUNT: usize`
- `ping_state_handler_table!(Self)` macro

### `transitions!` — Transition Rules

```rust
transitions![
    // Message pattern → actions + guard
    PingPongMsg::Pong(pong) => {
        actions [Self::log_pong, Self::forward_ping]
        guard(ctx, results) {
            results.any_failed()    => PingState::Error,
            ctx.round() >= MAX      => PingState::Done,
            _                       => stay,
        }
    },
    
    // Multiple patterns
    WorkerCtrl::AddPeer(_) | WorkerCtrl::RemovePeer(_) => {
        actions [Self::handle_ctrl]
        stay
    },
    
    // Simple transition
    PoolMsg::SpawnWorker(_) => {
        actions [Self::spawn_worker]
        transition PoolState::Active
    },
    
    // Guard only
    CounterMsg::Tick(_) => {
        actions [Self::count_tick]
        guard(ctx, _results) {
            ctx.count() >= MAX => CounterState::Done,
            _                  => stay,
        }
    },
]
```

**Body options:**
- `transition State` — change state
- `stay` — absorb event, keep state
- `reset` — exit to Init
- `guard(ctx, results) { ... }` — conditional decision

## StateFns Structure

```rust
const ACTIVE_FNS: StateFns<Self> = StateFns {
    on_entry: &[increment_round, send_ping],  // fn(&mut Ctx)
    on_exit: &[cancel_timer],                  // fn(&mut Ctx)
    transitions: transitions![...],            // &'static [StateRule<Self>]
};
```

**Entry/exit functions:** `fn(&mut Ctx)` — infallible, no event access

**Transition actions:** `fn(&mut Ctx, &Event) -> ActionResult`

## MachineSpec Trait

```rust
impl<R, B> MachineSpec for MySpec<R, B>
where
    R: BloxRuntime,
    B: MyBehavior + 'static,
{
    type State = MyState;
    type Event = MyEvent;
    type Ctx = MyCtx<R, B>;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<MyMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = my_state_handler_table!(Self);

    fn initial_state() -> MyState { MyState::Ready }
    
    fn is_terminal(state: &MyState) -> bool {
        matches!(state, MyState::Done)
    }
    
    fn is_error(state: &MyState) -> bool {
        matches!(state, MyState::Error)
    }
    
    fn on_init_entry(ctx: &mut MyCtx<R, B>) {
        // Reset context state
    }
}
```

**Optional methods:**
- `is_error` — marks fault states for supervisor intervention
- `on_init_entry` — reset logic when entering engine-implicit Init

## Common Patterns

### Timer Setup

```rust
// In actions crate
pub fn set_timer<R, C, M>(ctx: &mut C, duration_ms: u64, target: &ActorRef<M, R>, msg: M) -> TimerId
where
    R: BloxRuntime,
    C: HasTimerRef<R> + HasSelfId,
{
    let id = TimerId::new();
    let _ = ctx.timer_ref().try_send(
        ctx.self_id(),
        TimerCommand::Set {
            id,
            duration_ms,
            target: target.clone(),
            msg: Envelope(ctx.self_id(), msg),
        },
    );
    id
}

// In blox crate, on_entry
fn schedule_pause_timer(ctx: &mut PingCtx<R, B>) {
    let id = set_timer(ctx, PAUSE_DURATION_MS, ctx.self_ref(), Resume {});
    ctx.set_current_timer(Some(id));
}

// In blox crate, on_exit
fn cancel_current_timer(ctx: &mut PingCtx<R, B>) {
    if let Some(id) = ctx.current_timer() {
        let _ = ctx.timer_ref().try_send(ctx.self_id(), TimerCommand::Cancel { id });
        ctx.set_current_timer(None);
    }
}
```

### Error Handling in Guards

```rust
PingPongMsg::Pong(_) => {
    actions [Self::send_ping]
    guard(ctx, results) {
        results.any_failed() => PingState::Error,
        _                    => stay,
    }
}
```

### Factory Injection (Dynamic Spawning)

```rust
// Pool context with factory
#[derive(BloxCtx)]
pub struct PoolCtx<R: BloxRuntime> {
    pub self_id: ActorId,
    pub self_ref: ActorRef<PoolMsg, R>,
    pub worker_factory: WorkerSpawnFn<R>,  // fn(ActorId, &ActorRef<PoolMsg, R>) -> (ActorRef<WorkerMsg, R>, ActorRef<WorkerCtrl<R>, R>)
    pub worker_refs: Vec<ActorRef<WorkerMsg, R>>,
}

// Binary provides factory
fn spawn_worker_tokio(pool_id: ActorId, pool_ref: &ActorRef<PoolMsg, TokioRuntime>) 
    -> (ActorRef<WorkerMsg, TokioRuntime>, ActorRef<WorkerCtrl<TokioRuntime>, TokioRuntime>) 
{
    let ((domain_ref,), domain_mbox) = bloxide_tokio::channels! { WorkerMsg(8) };
    let ((ctrl_ref,), ctrl_mbox) = bloxide_tokio::channels! { WorkerCtrl<TokioRuntime>(4) };
    let worker_id = domain_ref.id();
    // ... spawn worker task ...
    (domain_ref, ctrl_ref)
}

let pool_ctx = PoolCtx::new(pool_id, pool_ref, spawn_worker_tokio);
```

### Peer Introduction

```rust
// In actions crate
pub fn introduce_new_worker<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasWorkerPeers<R>,
{
    let new_peer = ctx.worker_refs().last().unwrap();
    for ctrl_ref in ctx.worker_ctrls() {
        let _ = ctrl_ref.try_send(
            ctx.self_id(),
            WorkerCtrl::AddPeer(AddWorkerPeer { peer_ref: new_peer.clone() }),
        );
    }
}
```

## Runtime Wiring (Tokio)

### Channel Creation

```rust
let ((actor_ref,), mbox) = bloxide_tokio::channels! { MyMsg(16) };
```

### Timer Service

```rust
let timer_ref = bloxide_tokio::spawn_timer!(8);
```

### Supervised Actor Task

```rust
bloxide_tokio::actor_task_supervised!(my_task, MySpec<TokioRuntime, MyBehavior>);

let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
bloxide_tokio::spawn_child!(
    group,
    my_task(machine, mbox, actor_id),
    ChildPolicy::Restart { max: 1 }
);
```

### Supervisor Task

```rust
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

let (children, notify_rx, control_rx) = group.finish();
let sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
sup_machine.dispatch(SupervisorEvent::Lifecycle(LifecycleCommand::Start));
supervisor_task(sup_machine, (notify_rx, control_rx)).await;
```

## Test Utilities

### TestRuntime

```rust
use bloxide_core::test_utils::TestRuntime;

// Create test machine
let ctx = MyCtx::new(TestRuntime::alloc_actor_id(), TestBehavior::default());
let mut machine = StateMachine::new(ctx);

// Dispatch events
machine.dispatch(MyEvent::Msg(Envelope(0, MyMsg::Foo {})));

// Check state
assert!(matches!(machine.current_state(), MachineState::State(MyState::Ready)));

// Check context
assert_eq!(machine.ctx().behavior.count(), 1);
```

### Virtual Clock (for timers)

```rust
use bloxide_timer::test_utils::VirtualClock;

let clock = VirtualClock::new(timer_rx);
clock.advance(100);  // Advance 100ms, fire ready timers
```

## Field Annotation Details

### Auto-detected fields

The `#[derive(BloxCtx)]` macro detects fields by naming convention:

| Field | Convention | Generated |
|-------|------------|-----------|
| `self_id: ActorId` | Field named exactly `self_id` | `impl HasSelfId` |
| `foo_ref: ActorRef<M, R>` | Field ends with `_ref` | `impl HasFooRef<R>` |
| `foo_factory: fn(...) -> ...` | Field ends with `_factory` | Constructor parameter only |

### `#[delegates(...)]` — Required for behavior fields

```rust
#[delegates(TraitA, TraitB)]
pub behavior: B,
```

Generates forwarding impls. Import `__delegate_TraitA` from the action crate.

### `#[ctor]` — Override auto-detection

Use `#[ctor]` when a field matches the naming convention but you *don't* want a trait impl generated:

```rust
#[derive(BloxCtx)]
pub struct PoolCtx<R: BloxRuntime> {
    pub self_id: ActorId,              // → impl HasSelfId (auto-detected)
    #[ctor]                            // → no HasSelfRef<R> trait (we don't need it)
    pub self_ref: ActorRef<PoolMsg, R>,
    #[ctor]                            // → no trait impl (we implement HasWorkerFactory manually)
    pub worker_factory: WorkerSpawnFn<R>,
}
```

**When to use `#[ctor]`:**
- Field matches `_ref` pattern but you don't need the accessor trait
- Field matches `_factory` pattern but you implement a different trait manually
- Any field that should be a constructor parameter without generating a trait
