use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::command::{TimerCommand, TimerId};

struct PendingTimer {
    id: TimerId,
    deadline_ms: u64,
    deliver: Box<dyn FnOnce() + Send>,
}

/// A sorted queue of pending timers. Shared across all runtimes.
///
/// Each runtime's timer service (`TimerService` impl) owns a `TimerQueue`
/// and drives it using the runtime's native timer primitive.
pub struct TimerQueue {
    timers: Vec<PendingTimer>,
}

impl TimerQueue {
    pub fn new() -> Self {
        Self { timers: Vec::new() }
    }

    /// Insert a new timer that expires at `now_ms + after_ms`.
    pub fn set(
        &mut self,
        id: TimerId,
        after_ms: u64,
        now_ms: u64,
        deliver: Box<dyn FnOnce() + Send>,
    ) {
        let deadline_ms = now_ms.saturating_add(after_ms);
        let pos = self
            .timers
            .iter()
            .position(|t| t.deadline_ms > deadline_ms)
            .unwrap_or(self.timers.len());
        self.timers.insert(
            pos,
            PendingTimer {
                id,
                deadline_ms,
                deliver,
            },
        );
    }

    /// Cancel a pending timer. Returns `true` if it was found and removed.
    pub fn cancel(&mut self, id: TimerId) -> bool {
        if let Some(pos) = self.timers.iter().position(|t| t.id == id) {
            self.timers.remove(pos);
            true
        } else {
            false
        }
    }

    /// Returns the deadline (in ms) of the earliest pending timer, if any.
    pub fn next_deadline(&self) -> Option<u64> {
        self.timers.first().map(|t| t.deadline_ms)
    }

    /// Returns `true` if there are no pending timers.
    pub fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }

    /// Remove and return all timers whose deadline has passed.
    ///
    /// Callbacks are returned in deadline order (earliest first).
    pub fn drain_expired(&mut self, now_ms: u64) -> Vec<Box<dyn FnOnce() + Send>> {
        let split = self
            .timers
            .iter()
            .position(|t| t.deadline_ms > now_ms)
            .unwrap_or(self.timers.len());
        self.timers.drain(..split).map(|t| t.deliver).collect()
    }

    /// Process a `TimerCommand`, dispatching to `set`, `cancel`, or signalling
    /// shutdown. Returns `true` when the caller should exit its service loop.
    pub fn handle_command(&mut self, cmd: TimerCommand, now_ms: u64) -> bool {
        match cmd {
            TimerCommand::Set {
                id,
                after_ms,
                deliver,
            } => {
                self.set(id, after_ms, now_ms, deliver);
                false
            }
            TimerCommand::Cancel { id } => {
                self.cancel(id);
                false
            }
            TimerCommand::Shutdown => true,
        }
    }
}

impl Default for TimerQueue {
    fn default() -> Self {
        Self::new()
    }
}
