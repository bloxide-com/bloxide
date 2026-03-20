# Start Here

Bloxide is easiest to understand if you keep three related mental models in your head at the same time. This page is a quick reference; see **`AGENTS.md`** for the full mental model explanations.

## Three Mental Models — Quick Reference

| Model | Use it to decide... | Canonical Source |
|---|---|---|
| **Three-layer principle** | Where new framework capabilities belong | `AGENTS.md`, `spec/architecture/00-layered-architecture.md` |
| **Five-layer application** | How to organize domain code | `AGENTS.md`, `spec/architecture/12-action-crate-pattern.md` |
| **Two-tier trait system** | Which traits blox code can see | `AGENTS.md`, `spec/architecture/00-layered-architecture.md` |

## Read In This Order

1. `README.md` — repo map and runnable examples
2. `AGENTS.md` — mental models, key invariants, where-to-find-things table
3. `skills/building-with-bloxide/SKILL.md` — end-to-end build workflow
4. `QUICK_REFERENCE.md` — decision trees and lookup tables when you're stuck

Then dive deeper as needed:
- `spec/architecture/02-hsm-engine.md` — `MachineSpec`, dispatch, Init/start/reset
- `spec/architecture/05-handler-patterns.md` — transition patterns and `transitions!` macro
- `spec/architecture/08-supervision.md` — supervisor patterns
- `spec/architecture/11-dynamic-actors.md` — dynamic spawning and factory injection

## Init, Start, and Reset

Machines begin in engine-implicit **Init** (no callbacks fire). They leave Init only when `start()` is called — either by `run_actor_to_completion` (unsupervised) or by the supervisor sending `LifecycleCommand::Start` (supervised). `reset()` returns the machine to Init; `on_init_entry` fires only on reset, not on first construction.

> **Canonical source**: `spec/architecture/02-hsm-engine.md` explains Init, Start, Reset, and the VirtualRoot lifecycle handling in detail.

## First Things To Remember

- Blox crates are generic over `R: BloxRuntime`; they never import runtime crates.
- Messages are plain data only; no `ActorRef` inside domain message enums.
- Side effects belong in action functions; guards stay pure and only decide the next state.
- Runtime crates implement Tier 2 capabilities so blox code can stay portable.

## First Commands

```bash
# Smallest runnable example
cargo run --example tokio-minimal-demo

# Tokio runtime demo
cargo run --example tokio-demo

# Embassy-on-std demo
cargo run --example embassy-demo

# Example blox tests
cargo test -p ping-blox --features std
```

If you are building a new blox, keep `skills/building-with-bloxide/SKILL.md` and `skills/building-with-bloxide/reference.md` open while you work.
