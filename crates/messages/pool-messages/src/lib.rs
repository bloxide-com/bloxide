// Copyright 2025 Bloxide, all rights reserved
//! Pure domain message types for the worker pool example.
//!
//! No runtime dependencies — only plain data (AGENTS.md invariant #3).
//! The spawn protocol types (`SpawnRequest`, `SpawnedWorker`), which carry
//! `ActorRef`s, live in the domain context crate `blox-ctx-pool-ref`.
#![no_std]

pub mod generated;
pub mod prelude;

pub use generated::{DoWork, PeerResult, PoolMsg, SpawnWorker, WorkDone, WorkerMsg};
