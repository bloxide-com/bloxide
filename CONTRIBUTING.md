# Contributing to Bloxide

> **NO BACKWARDS COMPATIBILITY.** This project does not preserve backwards
> compatibility — not for APIs, CLIs, config formats, terminology, or
> architecture layers. When something changes, update every call site, every
> doc, and every fixture in the same change, and delete the old form
> completely. Never add legacy aliases, deprecation periods, compatibility
> shims, feature bridges, or "kept for backwards compatibility" notes.

## Development Setup

Run the main CI matrix locally using the CLI from this checkout:

```bash
./scripts/ci.sh
# Individual checks also use the current sources:
cargo run -p cargo-blox -- blox lint
cargo run -p cargo-blox -- blox test
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

`scripts/ci.sh` builds both workspaces, then runs `cargo blox ci`:

- Copyright header check and TOML/spec/documentation lint
- `cargo blox build` (runs codegen first) + feature-matrix `cargo check` runs
- `cargo fmt --check`
- `cargo clippy --all-targets -- -W warnings -D warnings`
- `cargo blox test` (root and generated workspaces, standalone impl crates)
- Isolated `bloxide-core` and `bloxide-embassy` tests with `std`
- `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS=-Dwarnings`
- `cargo deny check` when cargo-deny is installed (otherwise explicitly skipped)

The workflow also runs separate optional-feature tests (`tracing` and `bloxide-log/log`),
round-trip verification, and coverage. Run these separately when relevant; the local
script does not claim coverage for those jobs.
