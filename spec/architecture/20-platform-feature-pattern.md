# Platform Feature Pattern

> Defines how Layer-2 capabilities are packaged and consumed. The supervisor is
> the reference consumer: it owns no messages, no context fields, and no action
> functions of its own — only topology. Tracked as issue #139.

## Problem Statement

Before this pattern, Layer-2 capabilities were consumed through four different
mechanisms with inconsistent ownership:

| Capability | Message set lived in | Consumer actions lived in |
|---|---|---|
| Timer | `bloxide-timer` ✓ | `bloxide-timer` ✓ (plus a duplicate helper crate, `blox-ctx-current-timer`) |
| Peers | `bloxide-peers` ✓ | `bloxide-peers` ✓ (plus a leftover `HasPeers` accessor trait) |
| Spawn | none (domain `Req` by design) | impl crate ✓ |
| Lifecycle | `bloxide-core` ✓ | `bloxide-core` ✓ |
| Child management | **`bloxide-supervisor`** ✗ | **`bloxide-supervisor`** ✗ |

The supervisor — conceptually just *an example consumer* of the
child-management feature — owned that feature's control plane
(`SupervisorControl`, `RegisterChild`, `RegisterDynamicChild`) and its action
functions. `bloxide-messaging` was a demo-specific ping/pong action crate under
a platform-sounding name. The result: no single rule answered "where does the
code for capability X live, and how does a blox consume it?"

## The Contract

A **platform feature crate** provides some subset of:

1. **Message set** — plain data, or `R`-generic control types where refs and
   handles must travel (e.g. `ChildCtrl<R>`, `PeerCtrl<M, R>`).
2. **Context field declarations** — the `[[context.uses]]` building blocks
   (field name, type, role) that bloxes compose.
3. **Consumer-side action functions** — free functions taking concrete params
   or an extracted payload (codegen `event_payload`), **never** the consumer's
   event enum type.
4. **Tier 2 capability trait** — where runtime support is required
   (`TimerService`, `SpawnCap`, `KillCapability`).
5. **Wiring helpers** — e.g. `ChildGroupBuilder`, `spawn_child`.

A **blox** consumes features only through:

- `[[context.uses]]` entries pointing at feature crates,
- `[[context.actions]]` entries pointing at feature-crate functions,
- `system.toml` wiring of the injected refs/factories.

No feature logic and no feature message sets in a blox crate.

### Rules

1. **Message-driven features own their message sets.** Timer owns
   `TimerCommand`; child management owns `ChildCtrl`; peers own `PeerCtrl`;
   lifecycle lives in `bloxide-core` because the run loop consumes it.
2. **Factory/capability exception.** Spawn has no platform message set: the
   spawn *request* payload is domain-typed (`SpawnFn<R, Req>`), the *output*
   (`SpawnOutput`) and registration glue (`ChildRegistrar`) are platform.
3. **Feature crates vs domain context crates.** Feature crates are
   application-independent infrastructure (timer, peers, child management).
   Domain context crates (`blox-ctx-rounds`, `blox-ctx-ticks`,
   `blox-ctx-pool-ref`) hold application-specific action functions and remain
   valid — they are the mechanism by which *domain* logic stays out of blox
   crates. The distinction is ownership of the *capability*, not of the logic.
4. **No accessor traits, ever** (invariant #14). Consumer-side helpers are
   free functions with concrete params.
5. **One home per capability.** Helpers for consuming a feature live in the
   feature crate — not in per-blox duplicates, not in a parallel `blox-ctx-*`
   crate wrapping the feature.

## Feature Inventory (target state)

| Feature crate | Message set | Context fields | Action functions | Tier 2 trait |
|---|---|---|---|---|
| `bloxide-core` | `LifecycleCommand`, `ChildLifecycleEvent`, `AbortCommand` | — | run loop, `report_outcome` | `StaticChannelCap`, `DynamicChannelCap`, `KillCapability` |
| `bloxide-timer` | `TimerCommand` | `timer_ref`, `current_timer` | `set_timer`, `cancel_timer`, `schedule_resume`, `cancel_timer_by_id` | `TimerService` |
| `bloxide-peers` | `PeerCtrl<M, R>` | `peers` | `introduce_peers`, `apply_peer_control`, `broadcast_to_peers` | — |
| `bloxide-spawn` | — (exception) | `spawn_fn` (ctor) | `spawn_child`, `ChildRegistrar` | `SpawnCap` |
| `bloxide-child-management` | `ChildCtrl<R>` (`RegisterChild`, `RegisterDynamicChild`, `HealthCheckTick`) | `children`, `child_notify`, `pending` | supervision action functions (moved from `bloxide-supervisor`) | — |

Reference consumer: `bloxide-supervisor` — `blox.toml`, generated spec, tests.
Nothing else.

## Moves (from pre-pattern state)

- `SupervisorControl` → renamed **`ChildCtrl<R>`**, moved to
  `bloxide-child-management::control` with `RegisterChild`,
  `RegisterDynamicChild`, and the `ChildRegistrar` impl
  (`SupervisorRegistrar`). `bloxide-child-management` depends on
  `bloxide-spawn` (acyclic: core ← spawn ← child-management ← supervisor).
- Supervisor action functions → `bloxide-child-management::actions`,
  converted from `event_arg` (whole `SupervisorEvent`) to `event_payload`
  extraction (`ChildLifecycleEvent` / `ChildCtrl` payloads).
- `blox-ctx-current-timer` → folded into `bloxide-timer::actions`; crate deleted.
- `bloxide-messaging` → dissolved; `send_ping` / `send_pong` /
  `send_initial_ping` move to a demo context crate; crate deleted.
- `HasPeers` accessor trait removed from `bloxide-peers`.
- `bloxide-supervisor` keeps: `blox.toml`, `src/generated/`, `src/tests.rs`,
  and the thin `lib.rs` spec glue. `control.rs` and `actions.rs` are deleted.

## Acceptance Criteria (issue #139)

- [ ] `bloxide-supervisor` contains no message definitions, no action
      functions, no hand-written context
- [ ] `SupervisorControl` gone; `ChildCtrl` lives in `bloxide-child-management`
- [ ] `bloxide-messaging` and `blox-ctx-current-timer` crates deleted
- [ ] `HasPeers` gone from `bloxide-peers`
- [ ] Full workspace builds; all tests pass; all four demos regenerate and run
- [ ] This spec written; AGENTS.md invariant added
