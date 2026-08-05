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
    // to a fixed epoch by measuring elapsed time from a lazy static. The epoch
    // uses tokio::time::Instant — the same clock as the sleep primitive below —
    // so a paused Tokio clock (e.g. in tests) cannot make the two diverge.
    use std::sync::OnceLock;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
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

#[cfg(test)]
mod tests {
    use bloxide_core::capability::DynamicChannelCap;
    use bloxide_core::messaging::ActorRef;
    use bloxide_timer::{next_timer_id, TimerCommand, TimerService, TIMER_ACTOR_ID};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::task::JoinHandle;
    use tokio::time::Duration;

    use crate::TokioRuntime;

    /// Spawn the timer service on a fresh command channel.
    fn spawn_timer_service() -> (ActorRef<TimerCommand, TokioRuntime>, JoinHandle<()>) {
        let id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (timer_ref, timer_stream) =
            <TokioRuntime as DynamicChannelCap>::channel::<TimerCommand>(id, 8);
        let service = tokio::spawn(<TokioRuntime as TimerService>::run_timer_service(
            timer_stream,
        ));
        (timer_ref, service)
    }

    /// Yield so the spawned service task gets polled and consumes any queued
    /// commands before the test advances the paused clock.
    async fn let_service_run() {
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
    }

    #[tokio::test(start_paused = true)]
    async fn set_fires_deliver_after_duration() {
        let (timer_ref, service) = spawn_timer_service();

        let fired = Arc::new(AtomicBool::new(false));
        let deliver_flag = Arc::clone(&fired);
        timer_ref
            .try_send(
                TIMER_ACTOR_ID,
                TimerCommand::Set {
                    id: next_timer_id(),
                    after_ms: 1_000,
                    deliver: Box::new(move || deliver_flag.store(true, Ordering::SeqCst)),
                },
            )
            .expect("set command fits in the channel");
        let_service_run().await;

        // Before the deadline the timer must not fire.
        tokio::time::advance(Duration::from_millis(999)).await;
        let_service_run().await;
        assert!(!fired.load(Ordering::SeqCst), "timer must not fire early");

        // At the deadline the delivery callback runs.
        tokio::time::advance(Duration::from_millis(1)).await;
        let_service_run().await;
        assert!(fired.load(Ordering::SeqCst), "timer fires at its deadline");

        // The queue is empty now; closing the channel ends the service loop.
        drop(timer_ref);
        service
            .await
            .expect("service exits when the command channel closes");
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_prevents_firing() {
        let (timer_ref, service) = spawn_timer_service();

        let fired = Arc::new(AtomicBool::new(false));
        let deliver_flag = Arc::clone(&fired);
        let id = next_timer_id();
        timer_ref
            .try_send(
                TIMER_ACTOR_ID,
                TimerCommand::Set {
                    id,
                    after_ms: 100,
                    deliver: Box::new(move || deliver_flag.store(true, Ordering::SeqCst)),
                },
            )
            .expect("set command fits in the channel");
        let_service_run().await;

        timer_ref
            .try_send(TIMER_ACTOR_ID, TimerCommand::Cancel { id })
            .expect("cancel command fits in the channel");
        let_service_run().await;

        // Well past the original deadline: the cancelled timer never fires.
        tokio::time::advance(Duration::from_millis(1_000)).await;
        let_service_run().await;
        assert!(
            !fired.load(Ordering::SeqCst),
            "cancelled timer must not fire"
        );

        drop(timer_ref);
        service
            .await
            .expect("service exits when the command channel closes");
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_drains_pending_timers_and_exits() {
        let (timer_ref, service) = spawn_timer_service();

        // A pending timer whose deadline has not yet been reached.
        let fired = Arc::new(AtomicBool::new(false));
        let deliver_flag = Arc::clone(&fired);
        timer_ref
            .try_send(
                TIMER_ACTOR_ID,
                TimerCommand::Set {
                    id: next_timer_id(),
                    after_ms: 1_000,
                    deliver: Box::new(move || deliver_flag.store(true, Ordering::SeqCst)),
                },
            )
            .expect("set command fits in the channel");
        let_service_run().await;

        timer_ref
            .try_send(TIMER_ACTOR_ID, TimerCommand::Shutdown)
            .expect("shutdown command fits in the channel");

        // The service loop drains the queue and returns; a timer that has not
        // expired yet is dropped without firing.
        service.await.expect("service exits on Shutdown");
        assert!(
            !fired.load(Ordering::SeqCst),
            "pending timer is dropped on shutdown, not fired"
        );

        // Advancing time afterwards cannot fire it either.
        tokio::time::advance(Duration::from_millis(10_000)).await;
        let_service_run().await;
        assert!(!fired.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_when_idle_exits() {
        let (timer_ref, service) = spawn_timer_service();

        // No pending timers: Shutdown arrives while the service is parked on
        // the empty-queue receive branch.
        timer_ref
            .try_send(TIMER_ACTOR_ID, TimerCommand::Shutdown)
            .expect("shutdown command fits in the channel");

        service.await.expect("idle service exits on Shutdown");
    }

    #[tokio::test(start_paused = true)]
    async fn service_exits_when_command_channel_closes() {
        let (timer_ref, service) = spawn_timer_service();

        // Arm a timer so the service is parked in the deadline/sleep branch
        // when the channel closes.
        let fired = Arc::new(AtomicBool::new(false));
        let deliver_flag = Arc::clone(&fired);
        timer_ref
            .try_send(
                TIMER_ACTOR_ID,
                TimerCommand::Set {
                    id: next_timer_id(),
                    after_ms: 1_000,
                    deliver: Box::new(move || deliver_flag.store(true, Ordering::SeqCst)),
                },
            )
            .expect("set command fits in the channel");
        let_service_run().await;

        // Dropping the last sender closes the stream; the service shuts down
        // gracefully without waiting for the pending deadline.
        drop(timer_ref);
        service
            .await
            .expect("service exits when all command senders are dropped");
        assert!(
            !fired.load(Ordering::SeqCst),
            "pending timer is dropped on channel close, not fired"
        );
    }
}
