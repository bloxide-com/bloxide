// Copyright 2025 Bloxide, all rights reserved
use core::future::poll_fn;
use core::pin::Pin;

use bloxide_core::messaging::Envelope;
use bloxide_timer::{TimerCommand, TimerQueue, TimerService};
use futures_core::Stream;
use tokio::time::{sleep_until, Duration, Instant};

use crate::channel::TokioStream;
use crate::TokioRuntime;

fn now_ms() -> u64 {
    // Tokio's Instant does not expose `as_millis()` directly; compute relative
    // to a fixed epoch by measuring elapsed time from a lazy static.
    use std::sync::OnceLock;
    static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(std::time::Instant::now);
    epoch.elapsed().as_millis() as u64
}

impl TimerService for TokioRuntime {
    async fn run_timer_service(mut stream: TokioStream<TimerCommand>) {
        let mut queue = TimerQueue::new();
        loop {
            match queue.next_deadline() {
                Some(deadline_ms) => {
                    let now = now_ms();
                    let remaining_ms = deadline_ms.saturating_sub(now);
                    let sleep_future =
                        sleep_until(Instant::now() + Duration::from_millis(remaining_ms));
                    tokio::select! {
                        biased;
                        maybe_env = poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)) => {
                            match maybe_env {
                                Some(Envelope(_, cmd)) => {
                                    let now = now_ms();
                                    if queue.handle_command(cmd, now) {
                                        for deliver in queue.drain_expired(now) {
                                            deliver();
                                        }
                                        return;
                                    }
                                    for deliver in queue.drain_expired(now) {
                                        deliver();
                                    }
                                }
                                // All timer_ref senders dropped — shut down gracefully.
                                None => return,
                            }
                        }
                        _ = sleep_future => {
                            let now = now_ms();
                            for deliver in queue.drain_expired(now) {
                                deliver();
                            }
                        }
                    }
                }
                None => {
                    let maybe_env = poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
                    match maybe_env {
                        Some(Envelope(_, cmd)) => {
                            let now = now_ms();
                            if queue.handle_command(cmd, now) {
                                for deliver in queue.drain_expired(now) {
                                    deliver();
                                }
                                return;
                            }
                            for deliver in queue.drain_expired(now) {
                                deliver();
                            }
                        }
                        // All timer_ref senders dropped — shut down gracefully.
                        None => return,
                    }
                }
            }
        }
    }
}
