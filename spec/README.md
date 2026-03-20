# Bloxide Spec Directory

This directory is the single source of truth for all architecture decisions, state machine designs, and blox contracts. Write the spec before writing code.

## Recommended Reading Order (New Blox Authors)

When you are new to the codebase, this order minimizes context-switching:

1. [architecture/00-layered-architecture.md](architecture/00-layered-architecture.md)
2. [architecture/02-hsm-engine.md](architecture/02-hsm-engine.md)
3. [architecture/12-action-crate-pattern.md](architecture/12-action-crate-pattern.md)
4. [architecture/05-handler-patterns.md](architecture/05-handler-patterns.md)
5. [architecture/06-actions.md](architecture/06-actions.md)
6. [architecture/09-application.md](architecture/09-application.md)

## Spec-Driven Development Workflow

```
1. Spec     →  Write / update the relevant doc in this directory
2. Review   →  Verify diagrams, transition tables, and acceptance criteria
3. Test     →  Write tests against the acceptance criteria using TestRuntime
4. Implement → Write the Rust code to pass the tests
5. Sync     →  Update diagrams here if implementation reveals spec errors
```

The spec is always ahead of or equal to the code. Never let code drift silently from the spec.

## Directory Structure

```
spec/
  README.md                          ← you are here
  architecture/
    00-layered-architecture.md       ← three-layer principle, two-tier trait system, decision rule
    01-system-architecture.md        ← layers, crate graph, separation of concerns
    02-hsm-engine.md                 ← MachineSpec, dispatch algorithm, LCA transitions
    03-actor-messaging.md            ← ActorRef, Envelope, message flow, rules
    04-static-wiring.md              ← initialization order, channels!/actor_task! macros
    05-handler-patterns.md           ← TransitionRule patterns and topology patterns
    06-actions.md                    ← actions composition model, bloxide-log crate, transitions! macro
    07-typed-mailboxes.md            ← Mailboxes trait, priority ordering
    08-supervision.md                ← lifecycle messages, ChildPolicy, GroupShutdown, SupervisedRunLoop
    09-application.md                ← wiring patterns, prelude imports, setup() example
    10-effects-and-capabilities.md   ← effects, capabilities, two-tier traits, timer-as-service
    11-dynamic-actors.md             ← dynamic actor creation, peer control, factory injection
    12-action-crate-pattern.md       ← action crate pattern, five-layer architecture
  templates/
    blox-spec.md                     ← copy this to start a new blox spec
  bloxes/
    ping.md                          ← spec for the Ping blox
    pong.md                          ← spec for the Pong blox
    (supervisor is documented in architecture/08-supervision.md)
```

## Quick Navigation

| I want to... | Go to |
|---|---|
| Understand the layered architecture and two-tier trait system | [architecture/00-layered-architecture.md](architecture/00-layered-architecture.md) |
| Understand the overall system | [architecture/01-system-architecture.md](architecture/01-system-architecture.md) |
| Understand how state machines work | [architecture/02-hsm-engine.md](architecture/02-hsm-engine.md) |
| Understand how actors communicate | [architecture/03-actor-messaging.md](architecture/03-actor-messaging.md) |
| Understand how actors are wired together | [architecture/04-static-wiring.md](architecture/04-static-wiring.md) |
| Understand handler patterns and topology patterns | [architecture/05-handler-patterns.md](architecture/05-handler-patterns.md) |
| Understand the actions / bloxide-log crate | [architecture/06-actions.md](architecture/06-actions.md) |
| Understand typed mailboxes and priority ordering | [architecture/07-typed-mailboxes.md](architecture/07-typed-mailboxes.md) |
| Understand the lifecycle / supervision model | [architecture/08-supervision.md](architecture/08-supervision.md) |
| See a complete wiring example | [architecture/09-application.md](architecture/09-application.md) |
| Understand effects, capabilities, and timers | [architecture/10-effects-and-capabilities.md](architecture/10-effects-and-capabilities.md) |
| Understand dynamic actors and factory injection | [architecture/11-dynamic-actors.md](architecture/11-dynamic-actors.md) |
| Understand the action crate pattern (five-layer architecture) | [architecture/12-action-crate-pattern.md](architecture/12-action-crate-pattern.md) |
| Create a new blox | Copy [templates/blox-spec.md](templates/blox-spec.md) to `spec/bloxes/<name>.md` |
| Read the Ping spec | [bloxes/ping.md](bloxes/ping.md) |
| Read the Pong spec | [bloxes/pong.md](bloxes/pong.md) |
| Read the Supervisor spec | [architecture/08-supervision.md](architecture/08-supervision.md) |
## Creating a New Blox

1. Copy `spec/templates/blox-spec.md` → `spec/bloxes/<your-blox-name>.md`
2. Fill in every section (delete the instructional blockquotes as you go)
3. Get the spec reviewed before creating any Rust code
4. Create crates under `crates/`: `crates/bloxes/<your-blox-name>/`, and when needed `crates/messages/<your-blox-name>-messages/`, `crates/actions/<your-blox-name>-actions/`, and an impl crate such as `crates/impl/<name>/` consumed by the wiring binary
5. Write unit tests in the blox crate using `TestRuntime`, one test per acceptance criterion
6. Implement `MachineSpec` to make the tests pass
7. Add wiring in the application crate

## Key Invariants

These must never be violated. Violating them silently breaks the architecture:

- `bloxide-core` must compile under `no_std` with zero OS or executor imports
- Blox crates must be generic over `R: BloxRuntime`; never import a runtime crate
- Message enums must contain plain data only — no `ActorRef`, no runtime receiver handles
- Shared message types live in a dedicated `*-messages` crate, not inside either blox crate
- `on_entry` and `on_exit` are `fn(&mut Ctx)` — infallible, no `Result`
- Only leaf states may be transition targets (enforced by `debug_assert` in the engine)
