// Copyright 2025 Bloxide, all rights reserved
//! Test runtime: in-memory channels and a manual virtual clock.
//!
//! `TestRuntime` implements `BloxRuntime` (from `bloxide-core`) and `SpawnCap`
//! (from `bloxide-spawn`) so it can be used as the `R` type parameter in unit
//! tests without an Embassy or Tokio executor.
//!
//! Timer simulation is intentionally not part of `TestRuntime` itself.
//! Tests that use timers should drive `TimerCommand`s into a `TimerQueue`
//! directly; `bloxide-timer`'s internal `VirtualClock` helper
//! (`crates/bloxide-timer/src/test_utils.rs`, `#[cfg(test)]`-only) shows the
//! pattern.
//!
//! # Fidelity model (issue #135)
//!
//! Channels model the semantics the runtimes provide in production:
//!
//! - **Capacity** — `channel(id, capacity)` bounds `try_send`: it fails with
//!   `TestTrySendError::Full` once `capacity` envelopes are queued.
//!   `capacity = 0` means every `try_send` fails (always-full channel).
//! - **Close semantics** — the channel tracks its sender count. When the last
//!   `TestSender` (including every `ActorRef` clone) is dropped, the receiver
//!   drains any queued envelopes and then returns `Poll::Ready(None)` — the
//!   all-streams-close behavior of issue #134. Dropping the last sender also
//!   wakes a pending receiver so it observes the close.
//! - **Receiver liveness** — dropping the `TestReceiver` closes the send side:
//!   `try_send` fails with `TestTrySendError::Closed` (Tokio semantics), which
//!   `try_send_error_is_closed` classifies as closed. This lets tests drive
//!   the confirm-before-record supervision paths that key off dead channels.
//! - **Observable kill** — `SpawnCap` uses `usize` handles (a monotonically
//!   increasing spawn id). `kill` records the id in a thread-local log;
//!   `drain_killed()` / `kill_count()` let tests assert the kill path fired.
//!
//! # Intentional gaps
//!
//! - `send_via` is **unbounded** (no backpressure) — action functions all use
//!   `try_send`, so `try_send` is the backpressure path under test.
//! - Spawned futures are **recorded, not executed** — `kill` only logs the id;
//!   it does not (and cannot) drop the recorded future.
//!
//! # `no_std` support
//!
//! The crate is `no_std` + `alloc`. With the default `std` feature the spawn
//! and kill logs are thread-local (parallel `cargo test` threads stay
//! isolated); with `--no-default-features --features alloc` they become
//! process-global logs behind a `spin::Mutex` (single-threaded harnesses).

#![no_std]

extern crate alloc;

#[cfg(any(test, feature = "std"))]
extern crate std;

pub mod prelude;

mod runtime;

pub use runtime::{
    drain_killed, drain_spawned, kill_count, spawned_count, TestReceiver, TestRuntime,
    TestSendError, TestSender, TestTrySendError,
};

#[cfg(test)]
mod tests;
