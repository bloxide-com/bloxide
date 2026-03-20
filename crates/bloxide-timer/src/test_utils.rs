// Copyright 2025 Bloxide, all rights reserved
use crate::{TimerCommand, TimerQueue};
use bloxide_core::test_utils::TestReceiver;

/// Deterministic timer harness for `TestRuntime`-based tests.
///
/// `VirtualClock` owns the timer command receiver, drains pending
/// `TimerCommand`s into a `TimerQueue`, and fires ready callbacks when time is
/// advanced. This keeps timer simulation out of `bloxide-core` while still
/// providing a reusable helper for blox tests.
pub struct VirtualClock {
    timer_rx: TestReceiver<TimerCommand>,
    queue: TimerQueue,
    now_ms: u64,
}

impl VirtualClock {
    pub fn new(timer_rx: TestReceiver<TimerCommand>) -> Self {
        Self {
            timer_rx,
            queue: TimerQueue::new(),
            now_ms: 0,
        }
    }

    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// Drain any pending `TimerCommand`s into the internal queue.
    pub fn drain_commands(&mut self) -> usize {
        let commands = self.timer_rx.drain_payloads();
        let count = commands.len();
        for cmd in commands {
            self.queue.handle_command(cmd, self.now_ms);
        }
        count
    }

    /// Advance virtual time, fire all ready timers, and return how many fired.
    pub fn advance(&mut self, delta_ms: u64) -> usize {
        self.drain_commands();
        self.now_ms = self.now_ms.saturating_add(delta_ms);

        let ready = self.queue.drain_expired(self.now_ms);
        let count = ready.len();
        for deliver in ready {
            deliver();
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::VirtualClock;
    use crate::{next_timer_id, TimerCommand};
    use bloxide_core::test_utils::TestRuntime;
    use bloxide_core::DynamicChannelCap;
    use std::boxed::Box;
    use std::sync::{Arc, Mutex};
    use std::vec;
    use std::vec::Vec;

    #[test]
    fn advance_fires_ready_timers_in_deadline_order() {
        let timer_id = <TestRuntime as DynamicChannelCap>::alloc_actor_id();
        let (timer_ref, timer_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<TimerCommand>(timer_id, 8);

        let fired = Arc::new(Mutex::new(Vec::new()));

        let fired_late = Arc::clone(&fired);
        timer_ref
            .try_send(
                timer_id,
                TimerCommand::Set {
                    id: next_timer_id(),
                    after_ms: 10,
                    deliver: Box::new(move || fired_late.lock().unwrap().push(2)),
                },
            )
            .unwrap();

        let fired_early = Arc::clone(&fired);
        timer_ref
            .try_send(
                timer_id,
                TimerCommand::Set {
                    id: next_timer_id(),
                    after_ms: 5,
                    deliver: Box::new(move || fired_early.lock().unwrap().push(1)),
                },
            )
            .unwrap();

        let mut clock = VirtualClock::new(timer_rx);

        assert_eq!(clock.advance(5), 1);
        assert_eq!(*fired.lock().unwrap(), vec![1]);
        assert_eq!(clock.advance(5), 1);
        assert_eq!(*fired.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn cancel_prevents_timer_delivery() {
        let timer_id = <TestRuntime as DynamicChannelCap>::alloc_actor_id();
        let (timer_ref, timer_rx) =
            <TestRuntime as DynamicChannelCap>::channel::<TimerCommand>(timer_id, 8);

        let fired = Arc::new(Mutex::new(Vec::new()));
        let fired_flag = Arc::clone(&fired);
        let id = next_timer_id();

        timer_ref
            .try_send(
                timer_id,
                TimerCommand::Set {
                    id,
                    after_ms: 5,
                    deliver: Box::new(move || fired_flag.lock().unwrap().push(1)),
                },
            )
            .unwrap();
        timer_ref
            .try_send(timer_id, TimerCommand::Cancel { id })
            .unwrap();

        let mut clock = VirtualClock::new(timer_rx);

        assert_eq!(clock.advance(5), 0);
        assert!(fired.lock().unwrap().is_empty());
    }
}
