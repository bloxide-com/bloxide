use core::future::poll_fn;
use core::pin::Pin;

use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};
use futures_core::Stream;

use bloxide_core::messaging::Envelope;
use bloxide_timer::{TimerCommand, TimerQueue, TimerService};

use crate::channel::EmbassyStream;
use crate::EmbassyRuntime;

fn now_ms() -> u64 {
    Instant::now().as_millis()
}

impl TimerService for EmbassyRuntime {
    async fn run_timer_service(mut stream: EmbassyStream<TimerCommand>) {
        let mut queue = TimerQueue::new();
        loop {
            match queue.next_deadline() {
                Some(deadline_ms) => {
                    let now = now_ms();
                    let remaining_ms = deadline_ms.saturating_sub(now);
                    match select(
                        poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)),
                        Timer::after(Duration::from_millis(remaining_ms)),
                    )
                    .await
                    {
                        Either::First(Some(Envelope(_, cmd))) => {
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
                        Either::First(None) => unreachable!("timer stream terminated unexpectedly"),
                        Either::Second(()) => {
                            let now = now_ms();
                            for deliver in queue.drain_expired(now) {
                                deliver();
                            }
                        }
                    }
                }
                None => {
                    let poll_result = poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
                    match poll_result {
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
                        None => unreachable!("timer stream terminated unexpectedly"),
                    }
                }
            }
        }
    }
}
