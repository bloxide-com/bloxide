# Contributing to Bloxide

## Development Setup

```bash
cargo build --all
cargo test --all
cargo fmt
cargo clippy --all -- -D warnings
```

## Spec-Driven Development

1. **Spec first** — Write/update `spec/bloxes/<name>.md` with state diagram, events, transitions
2. **Generate** — Run `cargo blox generate` to regenerate boilerplate from `blox.toml`
3. **Tests next** — Write `TestRuntime`-based tests per acceptance criteria
4. **Then code** — Implement `MachineSpec` to pass tests
5. **Keep in sync** — Update spec if implementation reveals spec errors

See `skills/building-with-bloxide/SKILL.md` for the full workflow.

## Key Invariants

Before modifying any code, review the **Key Invariants** section in `AGENTS.md`. These are architectural constraints that must never be violated.

## Code Style

- All code files must include the copyright header:
  ```rust
  // Copyright 2025 Bloxide, all rights reserved
  ```

## CI Checks

- Copyright header check
- `cargo fmt --check`
- `cargo clippy --all -- -D warnings`
- `cargo test --all`
- `cargo doc --all --no-deps`
