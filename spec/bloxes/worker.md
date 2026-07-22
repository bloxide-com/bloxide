# Blox Spec: `Worker`

## Purpose

The Worker actor demonstrates:
- **Priority mailbox handling**: Ctrl stream polled before domain stream
- **Peer accumulation**: Receives `AddPeer` commands before `DoWork`
- **Result broadcast**: Sends result to all peers before notifying pool
- **Self-suspend via Guard::Stop**: When work is done, the guard returns `Guard::Stop` — the actor self-suspends to `Init` and the runtime reports `ChildLifecycleEvent::Stopped` to the pool

Workers are spawned dynamically by the Pool actor.

## Crate Location

- Blox crate: `crates/bloxes/worker/`
- Messages crate: `crates/messages/pool-messages/` (shared with Pool)
- Actions crate: `crates/actions/pool-actions/` (shared with Pool)
- No impl crate needed — behavior is simple enough for blox-internal state

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> Waiting : dispatch(Start)
    Waiting --> [*] : WorkerMsg::DoWork : Guard::Stop
```

> `[Init]` is engine-implicit. `Waiting` is a leaf state.

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for `dispatch(Start)`; `on_init_entry` clears peers and task state |
| `Waiting` | leaf, initial | Accumulating peer introductions; awaiting work assignment |

## Events

| Event | Handled by | Rule pattern | Guard outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| `PeerCtrl::AddPeer(_)` | `Waiting` | Action-Then-Stay | `Stay` | `apply_worker_ctrl` |
| `WorkerMsg::DoWork(_)` | `Waiting` | Action-Then-Guard | `Stop` | `process_work` |
| `WorkerMsg::PeerResult(_)` | `Waiting` | Sink | `Stay` | none (absorbed) |
| any unhandled | root (no rules) | — | dropped | none |

## Priority Mailbox Ordering

The Worker's `Mailboxes` tuple is ordered for priority poll:

```rust
type Mailboxes<Rt: BloxRuntime> = (R::Stream<PeerCtrl<WorkerMsg, R>>, R::Stream<WorkerMsg>);
//                                  ^-- index 0 (ctrl)                     ^-- index 1 (domain)
```

The runtime polls index 0 first, ensuring all `AddPeer` commands arrive before `DoWork`.

> The behavior type parameter `B` must implement `HasWorkerPeers<R>`. This trait provides the concrete peer vector that `PeerCtrl::AddPeer` appends to.

## Context

```rust
#[derive(BloxCtx)]
pub struct WorkerCtx<R: BloxRuntime> {
    #[self_id]
    pub self_id: ActorId,
    #[provides(HasPoolRef<R>)]
    pub pool_ref: ActorRef<PoolMsg, R>,
    #[delegates(HasCurrentTask)]
    pub task_id: u32,
    #[delegates(HasCurrentTask)]
    pub result: u32,
    #[delegates(HasWorkerPeers<R>)]
    pub peers: Vec<ActorRef<WorkerMsg, R>>,
}
```

| Field | Type | Annotation | Description |
|-------|------|------------|-------------|
| `self_id` | `ActorId` | `#[self_id]` | Actor identity |
| `pool_ref` | `ActorRef<PoolMsg, R>` | `#[provides(HasPoolRef<R>)]` | Reference to parent pool |
| `task_id` | `u32` | `#[delegates(HasCurrentTask)]` | Assigned task ID |
| `result` | `u32` | `#[delegates(HasCurrentTask)]` | Computed result |
| `peers` | `Vec<...>` | `#[delegates(HasWorkerPeers<R>)]` | Introduced peer refs |

## Message Contracts

### Receives

| Variant | Stream | Payload | Source |
|---------|--------|---------|--------|
| `PeerCtrl::AddPeer(ActorRef<WorkerMsg, R>)` | Ctrl (index 0) | peer ref | Pool (via introduce_peers) |
| `WorkerMsg::DoWork(DoWork { task_id })` | Domain (index 1) | task ID | Pool |
| `WorkerMsg::PeerResult(PeerResult { from_id, result })` | Domain (index 1) | peer result | Other workers |

### Sends

| Target | Message | When |
|--------|---------|------|
| All peers | `WorkerMsg::PeerResult(...)` | transition actions (before `Guard::Stop`) via `broadcast_to_peers` |
| `pool_ref` | `PoolMsg::WorkDone(...)` | transition actions (before `Guard::Stop`) via `notify_pool_done` |

## Entry / Exit Actions

| State | on_entry | on_exit |
|-------|----------|---------|
| `[Init]` (engine) | clear peers, set task_id=0, result=0 | — |
| `Waiting` | `log_waiting` | — |
| `Waiting` → `Guard::Stop` | `log_done`, `broadcast_to_peers`, `notify_pool_done` (in transition actions) | — |

## Acceptance Criteria

- [ ] `dispatch(WorkerEvent::Lifecycle(LifecycleCommand::Start))` exits Init and enters `Waiting`
- [ ] `PeerCtrl::AddPeer` in `Waiting` adds peer to list, stays in `Waiting`
- [ ] `WorkerMsg::DoWork` in `Waiting` sets task_id and result, runs transition actions (broadcast, notify), then `Guard::Stop` self-suspends to Init
- [ ] Transition actions broadcast result to all accumulated peers (before Guard::Stop)
- [ ] Transition actions send `WorkDone` to pool (before Guard::Stop)
- [ ] Ctrl stream is polled with higher priority than domain stream

## Acceptance Criteria → Test Mapping

| Acceptance Criterion | Test Function |
|---|---|
| `dispatch(LifecycleCommand::Start)` enters Waiting | `test_start_enters_waiting()` |
| AddPeer accumulates | `test_add_peer_accumulates()` |
| DoWork triggers Guard::Stop (with broadcast/notify actions) | `test_do_work_stops()` |
| Transition actions broadcast to peers | `test_broadcast_to_peers()` |
| Transition actions notify pool | `test_notify_pool_done()` |

## Action Crate Dependencies

| Trait | From crate | Implemented by |
|-------|-----------|----------------|
| `HasPoolRef<R>` | `pool-actions` | Generated via `#[provides]` |
| `HasCurrentTask` | `pool-actions` | Generated via `#[delegates]` |
| `HasWorkerPeers<R>` | `pool-actions` | Generated via `#[delegates]` |

## Related Docs

- See `spec/bloxes/pool.md` for the pool perspective
- See `spec/architecture/07-typed-mailboxes.md` for priority ordering
- See `spec/architecture/11-dynamic-actors.md` for peer introduction
