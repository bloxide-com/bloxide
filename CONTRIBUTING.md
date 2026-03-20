# Contributing to Bloxide

Thank you for your interest in contributing to Bloxide! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Documentation](#documentation)
- [Pull Request Process](#pull-request-process)

## Code of Conduct

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). Please be respectful and constructive in all interactions.

## Getting Started

### Architecture Overview

Bloxide is built on three mental models that you should understand before contributing:

1. **Three-layer principle** — Runtime primitives (Layer 1), standard library crates (Layer 2), bloxes (Layer 3)
2. **Five-layer application structure** — Messages, Actions, Impl, Blox, Binary
3. **Two-tier trait system** — Blox-facing traits (Tier 1), runtime-facing capabilities (Tier 2)

Read `START_HERE.md` and `AGENTS.md` for detailed explanations.

### Key Invariants

Before modifying any code, review the **Key Invariants** section in `AGENTS.md`. These are architectural constraints that must never be violated:

- `bloxide-core` is `no_std` — no OS/Tokio/Embassy imports
- Blox crates are runtime-agnostic (generic over `R: BloxRuntime`)
- Messages contain plain data only (no `ActorRef` in domain types)
- And more...

## Development Setup

### Prerequisites

- Rust 2021 edition (see `Cargo.toml` for minimum version)
- `cargo` with `fmt` and `clippy` components

### Building

```bash
# Build all crates
cargo build --all

# Build with no_std (core crates)
cargo build -p bloxide-core --no-default-features
```

### Running Examples

```bash
# Minimal single-actor example
cargo run --example tokio-minimal-demo

# Ping-pong with supervision and timers
RUST_LOG=trace cargo run --example tokio-demo

# Worker pool with dynamic spawning
RUST_LOG=info cargo run --example tokio-pool-demo

# Embassy runtime (std target)
RUST_LOG=trace cargo run --example embassy-demo
```

## Making Changes

### Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy --all -- -D warnings` and fix all issues
- All code files must include the copyright header:
  ```rust
  // Copyright 2025 Bloxide, all rights reserved
  ```

### Spec-Driven Development

When adding or modifying bloxes:

1. **Spec first** — Write/update `spec/bloxes/<name>.md` with state diagram, events, transitions
2. **Tests next** — Write `TestRuntime`-based tests per acceptance criteria
3. **Then code** — Implement `MachineSpec` to pass tests
4. **Review** — Verify impl matches spec; update tests if gaps found
5. **Keep in sync** — Update spec diagrams if implementation reveals spec errors

See `skills/building-with-bloxide/SKILL.md` for the full workflow.

### Layer Organization

When adding new functionality, follow the five-layer architecture:

| Layer | Location | Contents |
|-------|----------|----------|
| Messages | `crates/messages/` | Plain data structs, no logic |
| Actions | `crates/actions/` | Accessor traits, behavior traits, generic functions |
| Impl | `crates/impl/` | Concrete behavior implementations |
| Blox | `crates/bloxes/` | State topology, context, MachineSpec |
| Binary | `examples/` | Channel creation, context construction, task spawning |

## Testing

### Running Tests

```bash
# Run all tests
cargo test --all

# Run tests for a specific crate
cargo test -p bloxide-core
cargo test -p ping-blox --features std

# Run with tracing output
RUST_LOG=trace cargo test --all -- --nocapture
```

### Writing Tests

- Use `TestRuntime` from `bloxide_core::test_utils` for no-executor testing
- Use `VirtualClock` from `bloxide_timer::test_utils` for timer tests
- Follow the patterns in `crates/bloxes/ping/src/tests.rs`

## Documentation

### Updating Documentation

- Architecture changes → update `spec/architecture/` documents
- New bloxes → create `spec/bloxes/<name>.md` using `spec/templates/blox-spec.md`
- API changes → update rustdoc comments and `skills/building-with-bloxide/reference.md`

### Building Documentation

```bash
cargo doc --all --no-deps --open
```

## Pull Request Process

1. **Fork and branch** — Create a feature branch from `main`
2. **Make changes** — Follow the guidelines above
3. **Run checks** — Ensure `cargo fmt`, `cargo clippy`, and `cargo test` all pass
4. **Update docs** — Keep specs and documentation in sync with code
5. **Submit PR** — Provide a clear description of the change and motivation
6. **CI must pass** — All CI checks must be green before merge

### CI Checks

The CI pipeline runs:
- Copyright header check
- `cargo fmt --check`
- `cargo clippy --all -- -D warnings`
- `cargo test --all`
- `cargo doc --all --no-deps`

### PR Template

```markdown
## Summary

Brief description of the change.

## Motivation

Why is this change needed?

## Changes

- List of files/areas modified

## Testing

How was this tested?

## Documentation

- [ ] Spec updated (if applicable)
- [ ] rustdoc comments updated
- [ ] Reference docs updated (if applicable)
```

---

Thank you for contributing to Bloxide! 🎉
