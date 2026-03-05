// Copyright 2025 Bloxide, all rights reserved
use core::sync::atomic::{AtomicUsize, Ordering};

use bloxide_core::messaging::ActorId;

use alloc::boxed::Box;

/// Unique identifier for a pending timer, returned by `set_timer`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct TimerId(usize);

impl TimerId {
    /// Returns the underlying ID as a `u64` for display and logging purposes.
    pub fn as_u64(self) -> u64 {
        self.0 as u64
    }
}

/// Messages sent to the timer service actor.
pub enum TimerCommand {
    /// Schedule a callback after `after_ms` milliseconds.
    Set {
        id: TimerId,
        after_ms: u64,
        deliver: Box<dyn FnOnce() + Send>,
    },
    /// Cancel a previously scheduled timer.
    Cancel { id: TimerId },
    /// Shut down the timer service. All pending timers are drained (expired
    /// ones fire their callbacks) and the service loop exits.
    Shutdown,
}

/// Sentinel sender ID stamped on `Envelope::from` when a timer callback fires.
/// Uses `0` because the `next_actor_id!()` counter starts at 1, so `0` is
/// permanently unoccupied by any actor channel allocated at compile time.
pub const TIMER_ACTOR_ID: ActorId = 0;

static NEXT_TIMER_ID: AtomicUsize = AtomicUsize::new(1);

/// Allocate the next globally unique `TimerId`.
pub fn next_timer_id() -> TimerId {
    TimerId(NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed))
}
