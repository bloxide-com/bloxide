// Copyright 2025 Bloxide, all rights reserved
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

#[cfg(all(test, feature = "std"))]
mod tests {
    //! `run_timer_service` driven through Set/Cancel/Shutdown on a background
    //! std thread. Embassy's std time driver (dev-dependency features
    //! `embassy-time/std` + `generic-queue-8`) self-initializes on first use,
    //! so the service future only needs `embassy_futures::block_on` — no
    //! executor setup. `block_on` busy-polls, but the tests are short-lived.
    use super::*;
    use alloc::boxed::Box;
    use bloxide_core::capability::StaticChannelCap;
    use bloxide_core::messaging::ActorRef;
    use bloxide_timer::{next_timer_id, TimerId, TIMER_ACTOR_ID};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration as StdDuration;

    fn spawn_timer_service() -> (
        ActorRef<TimerCommand, EmbassyRuntime>,
        thread::JoinHandle<()>,
    ) {
        let (timer_ref, stream) =
            <EmbassyRuntime as StaticChannelCap>::channel::<TimerCommand, 8>(TIMER_ACTOR_ID);
        let handle = thread::spawn(move || {
            embassy_futures::block_on(<EmbassyRuntime as TimerService>::run_timer_service(stream));
        });
        (timer_ref, handle)
    }

    fn join_with_timeout(handle: thread::JoinHandle<()>) {
        for _ in 0..100 {
            if handle.is_finished() {
                handle.join().expect("timer service panicked");
                return;
            }
            thread::sleep(StdDuration::from_millis(10));
        }
        panic!("timer service did not exit within 1s of Shutdown");
    }

    fn shutdown_and_join(
        timer_ref: &ActorRef<TimerCommand, EmbassyRuntime>,
        handle: thread::JoinHandle<()>,
    ) {
        timer_ref
            .try_send(TIMER_ACTOR_ID, TimerCommand::Shutdown)
            .expect("shutdown command fits");
        join_with_timeout(handle);
    }

    #[test]
    fn set_timer_fires_its_command() {
        let (timer_ref, handle) = spawn_timer_service();
        let (fired_tx, fired_rx) = mpsc::channel::<TimerId>();
        let id = next_timer_id();
        timer_ref
            .try_send(
                TIMER_ACTOR_ID,
                TimerCommand::Set {
                    id,
                    after_ms: 30,
                    deliver: Box::new(move || {
                        let _ = fired_tx.send(id);
                    }),
                },
            )
            .expect("set command fits");
        let got = fired_rx
            .recv_timeout(StdDuration::from_secs(2))
            .expect("timer should fire");
        assert_eq!(got, id);
        shutdown_and_join(&timer_ref, handle);
    }

    #[test]
    fn cancel_prevents_firing() {
        let (timer_ref, handle) = spawn_timer_service();
        let (fired_tx, fired_rx) = mpsc::channel::<()>();
        let id = next_timer_id();
        timer_ref
            .try_send(
                TIMER_ACTOR_ID,
                TimerCommand::Set {
                    id,
                    after_ms: 50,
                    deliver: Box::new(move || {
                        let _ = fired_tx.send(());
                    }),
                },
            )
            .expect("set command fits");
        // FIFO ordering on the channel guarantees Set is handled first.
        timer_ref
            .try_send(TIMER_ACTOR_ID, TimerCommand::Cancel { id })
            .expect("cancel command fits");
        assert!(
            fired_rx
                .recv_timeout(StdDuration::from_millis(300))
                .is_err(),
            "cancelled timer must not fire"
        );
        shutdown_and_join(&timer_ref, handle);
    }

    #[test]
    fn shutdown_drains_and_exits() {
        let (timer_ref, handle) = spawn_timer_service();
        let (fired_tx, fired_rx) = mpsc::channel::<()>();
        let id = next_timer_id();
        // An already-expired timer queued ahead of Shutdown still fires —
        // the service drains expired timers before exiting its loop.
        timer_ref
            .try_send(
                TIMER_ACTOR_ID,
                TimerCommand::Set {
                    id,
                    after_ms: 0,
                    deliver: Box::new(move || {
                        let _ = fired_tx.send(());
                    }),
                },
            )
            .expect("set command fits");
        timer_ref
            .try_send(TIMER_ACTOR_ID, TimerCommand::Shutdown)
            .expect("shutdown command fits");
        fired_rx
            .recv_timeout(StdDuration::from_secs(2))
            .expect("expired timer must fire before the service exits");
        join_with_timeout(handle);
    }
}
