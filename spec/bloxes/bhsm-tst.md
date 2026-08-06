# Blox Spec: `BhsmTst`

## Purpose

The BhsmTst (Bloxide HSM Test) actor is a pedagogical demonstration of deep hierarchical state machine mechanics. It exercises every transition topology: self-transitions, parent→child transitions, cross-sibling transitions, deep cross-subtree transitions, and top-level catch-all transitions — the classic QHsmTst topology from Miro Samek's "Practical UML Statecharts". (The original console example printed a trace on every entry/exit; in bloxide the entry/exit actions are no-ops from the shared `blox-ctx-noop` context crate — the blox exists to prove the engine's topology handling.)

## Crate Location

- Blox crate: `bloxes/bhsm-tst/`
- Messages crate: `crates/messages/bhsm-tst-messages/`
- Context crate: `crates/context/blox-ctx-noop/` — shared no-op action functions (no mutable state; bhsm-tst is its first consumer)

## State Hierarchy

```mermaid
stateDiagram-v2
    [*] --> S11 : dispatch(Start)

    state S {
        state S1 {
            S11
        }
        state S2 {
            state S21 {
                S211
            }
        }
    }

    S11 --> S11 : A [self]
    S11 --> S11 : B [self]
    S11 --> S211 : D [LCA=S]
    S1 --> S211 : C [LCA=S]
    S21 --> S211 : E [LCA=S21]
    S211 --> S11 : F [LCA=S]
    S21 --> S11 : G [LCA=S]
    S --> S11 : H [reset]
    S --> Error : K [error]
    S --> [*] : X : Decision::Stop
```

> `[Init]` is engine-implicit (not in the `BhsmTstState` enum). The actor enters Init at construction and waits. `dispatch(BhsmTstEvent::Lifecycle(LifecycleCommand::Start))` exits Init and enters `S→S1→S11`. `dispatch(BhsmTstEvent::Lifecycle(LifecycleCommand::Reset))` goes **directly** to `initial_state()` (S11 via the S→S1→S11 entry chain) — it skips Init entirely and `on_init_entry` does NOT fire.
> `S`, `S1`, `S2`, `S21` are composite states (never active). `S11`, `S211`, `Error` are leaf states.

## States

| State | Kind | Description |
|-------|------|-------------|
| `[Init]` | engine-implicit | Waiting for `dispatch(Start)` |
| `S` | composite, top-level | Catch-all for H, I, K, X events |
| `S1` | composite | Parent of S11. Handles C (bubbled from S11) |
| `S11` | leaf, initial | Initial state. Handles A, B, D. |
| `S2` | composite | Parent of S21 |
| `S21` | composite | Parent of S211. Handles E, G (both bubbled from S211) |
| `S211` | leaf | Handles F (deep cross back) |
| `Error` | leaf, error | `is_error()` returns true. Supervisor restarts. |

## Events

| Event | Handled by | Rule pattern | Guard outcome | Side effects |
|-------|-----------|--------------|--------------|--------------|
| `A` | `S11` | Pure Transition | `Decision::Transition(S11)` | self-transition at leaf (LCA=`S1`) |
| `B` | `S11` | Pure Transition | `Decision::Transition(S11)` | same mechanics, different chain (LCA=`S1`) |
| `C` | `S1` (bubbled from `S11`) | Bubble | `Decision::Transition(S211)` | cross-sibling via parent (LCA=`S`) |
| `D` | `S11` | Pure Transition | `Decision::Transition(S211)` | deep cross-subtree (LCA=`S`) |
| `E` | `S21` (bubbled from `S211`) | Bubble | `Decision::Transition(S211)` | parent→child, single ancestor (LCA=`S21`) |
| `F` | `S211` | Pure Transition | `Decision::Transition(S11)` | deep cross back (LCA=`S`) |
| `G` | `S21` (bubbled from `S211`) | Bubble | `Decision::Transition(S11)` | mid-level cross (LCA=`S`) |
| `H` | `S` | Pure Transition | `Decision::Transition(S11)` | top-level reset (LCA=`S`) |
| `I` | `S` | Sink | `Decision::Stay` | top-level absorb |
| `K` | `S` | Pure Transition | `Decision::Transition(Error)` | error state (LCA=None — full exit chain); runtime reports `Failed` |
| `X` | `S` | Pure Guard | `Decision::Stop` | self-suspend to Init; supervisor notified via `Stopped` |
| any unhandled | root (no rules) | — | dropped | none |

Lifecycle control (`Start`, `Reset`, `Stop`) is handled by the runtime — these do not appear as domain events.

## Context

```rust
pub struct BhsmTstCtx {
    pub self_id: ActorId,
}
```

No state fields — this is a pure topology demonstration. `BhsmTstCtx::new(actor_id)` is the only constructor.

## Entry / Exit Actions

Every state declares `on_entry` and `on_exit` hooks; they are no-ops (trace
prints `{state}-ENTRY;` / `{state}-EXIT;` in the original Samek example). All
17 declared actions — the entry/exit hooks plus the three transition actions
(`s_i`, `s11_a`, `s11_b`) — resolve to `noop()` from the shared `blox-ctx-noop`
context crate (`crate = "blox_ctx_noop"` + `fn_name = "noop"` +
`returns = "ActionResult"` in each `[[context.actions]]` entry), demonstrating
the composable context-crate pattern (spec 13). The hooks exist so the topology exercises the engine's full
exit/entry chain machinery.

| State | on_entry | on_exit |
|-------|----------|---------|
| `[Init]` (engine) | — | — |
| `S` | (no-op) | (no-op) |
| `S1` | (no-op) | (no-op) |
| `S11` | (no-op) | (no-op) |
| `S2` | (no-op) | (no-op) |
| `S21` | (no-op) | (no-op) |
| `S211` | (no-op) | (no-op) |
| `Error` | (no-op) | (no-op) |

## LCA Exit/Entry Examples

### `S11 → S211` via event D (deep cross-subtree, LCA = S)
```
source_path: [S, S1, S11]
target_path: [S, S2, S21, S211]
LCA = S (index 0)

Exit:   S11.on_exit  ← s11-EXIT;
        S1.on_exit   ← s1-EXIT;
Entry:  S2.on_entry  ← s2-ENTRY;
        S21.on_entry ← s21-ENTRY;
        S211.on_entry← s211-ENTRY;
```
> `S.on_exit` / `S.on_entry` do NOT fire — the LCA state itself never exits
> or re-enters (spec 01). Only when source and target share NO user ancestor
> (LCA = None, e.g. `K` → `Error`) do the full chains fire.

### `S11 → S211` via event C (bubbled to S1, LCA = S)
```
source_path: [S, S1, S11]
target_path: [S, S2, S21, S211]
LCA = S (index 0)

Exit:   S11.on_exit  ← s11-EXIT;
        S1.on_exit   ← s1-EXIT;
Entry:  S2.on_entry  ← s2-ENTRY;
        S21.on_entry ← s21-ENTRY;
        S211.on_entry← s211-ENTRY;
```
> `S.on_exit` does NOT fire — this is an intra-subtree transition.

### `S211 → S211` via event E (bubbled to S21, LCA = S21)
```
source_path: [S, S2, S21, S211]
target_path: [S, S2, S21, S211]
LCA = S21 (index 2)

Exit:   S211.on_exit ← s211-EXIT;
Entry:  S211.on_entry← s211-ENTRY;
```

### `any → S11` via event H (top-level reset, LCA = S)
```
source_path: [S, ...]  (from any substate)
target_path: [S, S1, S11]
LCA = S (S is a common ancestor of every in-S substate and S11)

Exit:   (chain from current leaf up to — but not including — S)
Entry:  S1.on_entry  ← s1-ENTRY;
        S11.on_entry ← s11-ENTRY;
```
> `S.on_exit` / `S.on_entry` do NOT fire — `H` is an intra-`S` transition.

### `any → Error` via event K
```
source_path: [S, ...]  (from any substate)
target_path: [Error]
LCA = None

Exit:   (full chain from current leaf up through S)
        S.on_exit    ← s-EXIT;
Entry:  Error.on_entry← error-ENTRY;  (is_error() → supervisor reports Failed)
```

### `any → Decision::Stop` via event X
```
source_path: [S, ...]  (from any substate)
target_path: [Init]  # Decision::Stop goes to Init
LCA = None

Exit:   (full chain from current leaf up through S)
        S.on_exit    ← s-EXIT;
Decision::Stop: fires exit chain from current state to root, enters Init. Supervisor reports Stopped.
```

## Acceptance Criteria

> Verified by `bloxes/bhsm-tst/tests/bhsm-tst.rs` (15 tests) —
> a recording spec over the **generated** topology asserts the exact
> exit/entry chain order per transition. Its action closures call
> `blox_ctx_noop::noop()` in the same shapes the system-level codegen emits
> (`|ctx| { noop(); }` for entry/exit, a bare `noop()` call returning
> `ActionResult` for transitions), so the crate-ified actions are compiled
> and exercised in-crate. Chains follow spec 01 LCA semantics:
> the LCA state itself never exits or re-enters; full chains fire only when
> `LCA = None` (e.g. `K` → `Error`, `X` → `Stop`).

- [x] `dispatch(Start)` enters `S11` through the `S→S1→S11` entry chain
- [x] `A` in `S11` self-transitions: exit `S11`, entry `S11`
- [x] `D` in `S11` cross-subtree to `S211`: exit `S11,S1`, entry `S2,S21,S211` (LCA=`S`)
- [x] `C` in `S11` bubbles to `S1`, transitions to `S211`: exit `S11,S1`, entry `S2,S21,S211`
- [x] `E` in `S211` bubbles to `S21`, transitions to `S211` (parent→child): exit `S211`, entry `S211`
- [x] `F` in `S211` cross back to `S11`: exit `S211,S21,S2`, entry `S1,S11` (LCA=`S`)
- [x] `G` in `S211` bubbles to `S21`, transitions to `S11`: exit `S211,S21,S2`, entry `S1,S11`
- [x] `H` from any state resets to `S11`: exit up to (not incl.) `S`, entry `S1,S11`
- [x] `I` at top level (`S`) is absorbed — stay, no transition
- [x] `K` from any state transitions to `Error`: `is_error()` returns true, runtime reports `Failed` (LCA=None — full exit chain)
- [x] `X` from any state triggers `Decision::Stop`: actor self-suspends to Init, supervisor notified via `Stopped`
- [x] `R` (LifecycleCommand::Reset) goes directly to `initial_state()` (S11 via S→S1→S11), skipping Init
- [x] `Q` (LifecycleCommand::Stop) sends actor to Init (suspended)
- [x] Unknown events bubble to root and are silently dropped

## blox.toml

The full declarative source is `bloxes/bhsm-tst/blox.toml`; representative excerpts:

```toml
# S: top-level catch-all — H→S11, I stays, K→Error, X→stop
[[topology.transitions]]
state = "S"
event = "BhsmTstMsg::H(_)"
target = "S11"

[[topology.transitions]]
state = "S"
event = "BhsmTstMsg::I(_)"
target = "stay"

[[topology.transitions]]
state = "S"
event = "BhsmTstMsg::K(_)"
target = "Error"

[[topology.transitions]]
state = "S"
event = "BhsmTstMsg::X(_)"
target = "stop"

# S1: C is handled HERE (bubbled up from S11), not in S21
[[topology.transitions]]
state = "S1"
event = "BhsmTstMsg::C(_)"
target = "S211"
```

Every state also declares `[[topology.entry]]` / `[[topology.exit]]` tracing actions (`s1_entry`, `s1_exit`, …) so the recording test spec can assert exact chain order.

## Open Questions

None currently.
