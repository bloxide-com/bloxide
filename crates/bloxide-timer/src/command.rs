// Copyright 2025 Bloxide, all rights reserved
#[cfg(not(target_has_atomic = "ptr"))]
use core::cell::Cell;
#[cfg(target_has_atomic = "ptr")]
use core::sync::atomic::{AtomicUsize, Ordering};

use bloxide_core::messaging::ActorId;
#[cfg(not(target_has_atomic = "ptr"))]
use critical_section::Mutex;

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

/// Sentinel sender ID stamped into the sender field of `Envelope` when a timer
/// callback fires.
/// Uses `0` because the `next_actor_id!()` counter starts at 1, so `0` is
/// permanently unoccupied by any actor channel allocated at compile time.
pub const TIMER_ACTOR_ID: ActorId = 0;

// Fallback to a critical-section-protected counter on targets that lack
// pointer-sized atomics, such as ESP32-C3's `riscv32imc-unknown-none-elf`.
#[cfg(target_has_atomic = "ptr")]
static NEXT_TIMER_ID: AtomicUsize = AtomicUsize::new(1);
#[cfg(not(target_has_atomic = "ptr"))]
static NEXT_TIMER_ID: Mutex<Cell<usize>> = Mutex::new(Cell::new(1));

/// Allocate the next globally unique `TimerId`.
pub fn next_timer_id() -> TimerId {
    #[cfg(target_has_atomic = "ptr")]
    {
        TimerId(NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed))
    }

    #[cfg(not(target_has_atomic = "ptr"))]
    {
        critical_section::with(|cs| {
            let next = NEXT_TIMER_ID.borrow(cs);
            let id = next.get();
            next.set(id.saturating_add(1));
            TimerId(id)
        })
    }
}
