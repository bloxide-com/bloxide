// Copyright 2025 Bloxide, all rights reserved
//! SpawnCap implementation for TestRuntime.
//!
//! Collects spawned futures in a thread-local Vec for inspection in tests.
//! Use `drain_spawned()` and `spawned_count()` to inspect spawned actors.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use std::cell::RefCell;
use std::thread_local;

use crate::capability::SpawnCap;
use bloxide_core::test_utils::TestRuntime;

type SpawnedVec = Vec<Pin<Box<dyn Future<Output = ()> + Send>>>;

thread_local! {
    static SPAWNED: RefCell<SpawnedVec> = RefCell::new(Vec::new());
}

impl SpawnCap for TestRuntime {
    fn spawn(future: impl Future<Output = ()> + Send + 'static) {
        SPAWNED.with(|s: &RefCell<SpawnedVec>| s.borrow_mut().push(Box::pin(future)));
    }
}

/// Drain all futures submitted via `SpawnCap::spawn` since the last drain.
pub fn drain_spawned() -> SpawnedVec {
    SPAWNED.with(|s: &RefCell<SpawnedVec>| s.borrow_mut().drain(..).collect())
}

/// Returns the number of futures submitted since the last drain.
pub fn spawned_count() -> usize {
    SPAWNED.with(|s: &RefCell<SpawnedVec>| s.borrow().len())
}
