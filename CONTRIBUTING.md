# Contributing to Bloxide

> **NO BACKWARDS COMPATIBILITY.** This project does not preserve backwards
> compatibility — not for APIs, CLIs, config formats, terminology, or
> architecture layers. When something changes, update every call site, every
> doc, and every fixture in the same change, and delete the old form
> completely. Never add legacy aliases, deprecation periods, compatibility
> shims, feature bridges, or "kept for backwards compatibility" notes.

## Development Setup

Run the full CI suite locally (same commands as the GitHub workflow):

```bash
./scripts/ci.sh          # all checks: copyright, build, fmt, clippy, tests, docs
./scripts/ci.sh lint     # only build / check / fmt / clippy
./scripts/ci.sh test     # only tests
```

## Spec-Driven Development

See `AGENTS.md` → "Development Workflow" and `skills/building-with-bloxide/SKILL.md`
for the full workflow (spec → generate → tests → implement → sync).

## Key Invariants

Before modifying any code, review the **Key Invariants** section in `AGENTS.md`. These are architectural constraints that must never be violated.

## Code Style

- All code files must include the copyright header:
  ```rust
  // Copyright 2025 Bloxide, all rights reserved
  ```

## CI Checks

`scripts/ci.sh` runs the same checks as CI:

- Copyright header check
- `cargo build` + feature-matrix `cargo check` runs
- `cargo fmt --check`
- `cargo clippy --all-targets -- -W warnings -D warnings`
- `cargo test`
- `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS=-Dwarnings`
