// Copyright 2025 Bloxide, all rights reserved
use bloxide_child_management::AbortCommand;
use bloxide_core::{
    engine::{DispatchOutcome, StateMachine},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    mailboxes::Mailboxes,
    messaging::{ActorId, Envelope},
    report_outcome,
    spec::MachineSpec,
};
use core::future::poll_fn;
use core::pin::Pin;
use core::task::Poll;
use futures_core::Stream;

use crate::{TokioRuntime, TokioSender, TokioStream};

// ── Unified run loop ─────────────────────────────────────────────────────────

/// Configuration for the unified [`run`] loop.
///
/// All fields are optional — the loop adapts to what is provided:
///
/// | Field | Effect when `Some` | Effect when `None` |
/// |-------|-------------------|--------------------|
/// | `lifecycle` | Polls lifecycle commands (Start/Stop/Reset) | No lifecycle stream |
/// | `abort` | Polls abort commands (cooperative self-termination) | No abort stream |
/// | `supervisor_notify` | Reports outcomes to supervisor | No outcome reporting |
/// | `auto_start` | Calls `handle_lifecycle(Start)` before entering the loop | Actor starts via lifecycle stream |
/// | `exit_on_stop` | `DispatchOutcome::Stopped` breaks the loop | Actor stays alive in Init, waiting for Start/Reset |
///
/// **Typical configurations:**
///
/// - **Root supervisor**: `exit_on_stop = true`, no lifecycle/abort/notify
/// - **Supervised child**: `lifecycle + supervisor_notify`, `exit_on_stop = false`
/// - **Supervised child with kill capability**: `lifecycle + abort + supervisor_notify`, `exit_on_stop = false`
/// - **Unsupervised actor**: `auto_start = true`, `exit_on_stop = true`
pub struct RunConfig<R: bloxide_core::capability::BloxRuntime> {
    /// Lifecycle command stream from the supervisor. `None` for unsupervised/root actors.
    pub lifecycle: Option<R::Stream<LifecycleCommand>>,
    /// Abort command stream for cooperative kill. `None` for static (NoKill) children.
    pub abort: Option<R::Stream<AbortCommand>>,
    /// Sender to report `ChildLifecycleEvent` to the supervisor. `None` for unsupervised/root.
    pub supervisor_notify: Option<R::Sender<ChildLifecycleEvent>>,
    /// Auto-start the actor before entering the loop. Use for unsupervised actors.
    pub auto_start: bool,
    /// Exit the loop when `DispatchOutcome::Stopped` is observed.
    /// `true` for root/unsupervised, `false` for supervised (stays alive in Init).
    pub exit_on_stop: bool,
}

impl<R: bloxide_core::capability::BloxRuntime> RunConfig<R> {
    /// Configuration for a root supervisor or root actor.
    ///
    /// No lifecycle stream, no abort, no supervisor notify.
    /// Exits on `Stopped` (program done signal from root supervisor).
    pub fn root() -> Self {
        Self {
            lifecycle: None,
            abort: None,
            supervisor_notify: None,
            auto_start: false,
            exit_on_stop: true,
        }
    }

    /// Configuration for a supervised child actor.
    ///
    /// Lifecycle stream + supervisor notify. Stays alive on `Stopped`.
    pub fn supervised(
        lifecycle: R::Stream<LifecycleCommand>,
        actor_id: ActorId,
        supervisor_notify: R::Sender<ChildLifecycleEvent>,
    ) -> Self {
        let _ = actor_id;
        Self {
            lifecycle: Some(lifecycle),
            abort: None,
            supervisor_notify: Some(supervisor_notify),
            auto_start: false,
            exit_on_stop: false,
        }
    }

    /// Configuration for a supervised child with kill capability.
    ///
    /// Lifecycle + abort + supervisor notify. Stays alive on `Stopped`.
    pub fn supervised_with_abort(
        lifecycle: R::Stream<LifecycleCommand>,
        abort: R::Stream<AbortCommand>,
        actor_id: ActorId,
        supervisor_notify: R::Sender<ChildLifecycleEvent>,
    ) -> Self {
        let _ = actor_id;
        Self {
            lifecycle: Some(lifecycle),
            abort: Some(abort),
            supervisor_notify: Some(supervisor_notify),
            auto_start: false,
            exit_on_stop: false,
        }
    }

    /// Configuration for an unsupervised actor that auto-starts and exits on stop.
    pub fn unsupervised() -> Self {
        Self {
            lifecycle: None,
            abort: None,
            supervisor_notify: None,
            auto_start: true,
            exit_on_stop: true,
        }
    }
}

// ── The single run loop ──────────────────────────────────────────────────────

/// The unified run loop for Tokio-based actors.
///
/// Polls lifecycle → abort → domain mailboxes in priority order, dispatches
/// events through the machine, and reports outcomes to the supervisor (if any).
/// Yields to the executor after each message to prevent task starvation.
///
/// The loop exits when:
/// - `exit_on_stop` is true and `DispatchOutcome::Stopped` is observed
/// - `DispatchOutcome::Aborted` is observed (always exits)
/// - `DispatchOutcome::Failed` is observed (always exits)
/// - Any polled stream returns `Poll::Ready(None)` (stream closed)
///
/// When `exit_on_stop` is false (supervised actors), `Stopped` is NOT terminal —
/// the actor self-suspends to Init and the task stays alive, waiting for a
/// future `Start` or `Reset` from the supervisor.
pub async fn run<S, M>(
    mut machine: StateMachine<S>,
    mut domain_mailboxes: M,
    config: RunConfig<TokioRuntime>,
    actor_id: ActorId,
) where
    S: MachineSpec + 'static,
    M: Mailboxes<S::Event>,
{
    // Optional auto-start for unsupervised actors
    if config.auto_start {
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
        if let Some(ref notify) = config.supervisor_notify {
            report_outcome::<S, TokioRuntime>(&outcome, actor_id, notify);
        }
        match &outcome {
            DispatchOutcome::Started(bloxide_core::MachineState::State(state))
                if S::is_error(state) =>
            {
                return;
            }
            DispatchOutcome::Failed => return,
            DispatchOutcome::Stopped if config.exit_on_stop => return,
            _ => {}
        }
    }

    let mut lifecycle_stream = config.lifecycle;
    let mut abort_stream = config.abort;
    let supervisor_notify = config.supervisor_notify;

    enum LoopAction {
        Continue,
        Stop,
    }

    loop {
        let action = poll_fn(|cx| {
            // 1. Lifecycle stream (highest priority)
            if let Some(ref mut ls) = lifecycle_stream {
                match Pin::new(ls).poll_next(cx) {
                    Poll::Ready(None) => return Poll::Ready(LoopAction::Stop),
                    Poll::Ready(Some(Envelope(_, cmd))) => {
                        let outcome = machine.handle_lifecycle(cmd);
                        if let Some(ref notify) = supervisor_notify {
                            report_outcome::<S, TokioRuntime>(&outcome, actor_id, notify);
                        }
                        match &outcome {
                            DispatchOutcome::Aborted | DispatchOutcome::Failed => {
                                return Poll::Ready(LoopAction::Stop);
                            }
                            DispatchOutcome::Stopped if config.exit_on_stop => {
                                return Poll::Ready(LoopAction::Stop);
                            }
                            _ => return Poll::Ready(LoopAction::Continue),
                        }
                    }
                    Poll::Pending => {}
                }
            }

            // 2. Abort stream (high priority — serviced before domain messages)
            if let Some(ref mut as_) = abort_stream {
                match Pin::new(as_).poll_next(cx) {
                    Poll::Ready(None) => return Poll::Ready(LoopAction::Stop),
                    Poll::Ready(Some(Envelope(_, AbortCommand::Abort { .. }))) => {
                        if let Some(ref notify) = supervisor_notify {
                            report_outcome::<S, TokioRuntime>(
                                &DispatchOutcome::Aborted,
                                actor_id,
                                notify,
                            );
                        }
                        return Poll::Ready(LoopAction::Stop);
                    }
                    Poll::Pending => {}
                }
            }

            // 3. Domain mailboxes
            match domain_mailboxes.poll_next(cx) {
                Poll::Ready(Some(event)) => {
                    let outcome = machine.dispatch(event);
                    if let Some(ref notify) = supervisor_notify {
                        report_outcome::<S, TokioRuntime>(&outcome, actor_id, notify);
                    }
                    match &outcome {
                        DispatchOutcome::Aborted | DispatchOutcome::Failed => {
                            Poll::Ready(LoopAction::Stop)
                        }
                        DispatchOutcome::Stopped if config.exit_on_stop => {
                            Poll::Ready(LoopAction::Stop)
                        }
                        _ => Poll::Ready(LoopAction::Continue),
                    }
                }
                Poll::Ready(None) => Poll::Ready(LoopAction::Stop),
                Poll::Pending => Poll::Pending,
            }
        })
        .await;

        match action {
            LoopAction::Continue => {
                tokio::task::yield_now().await;
            }
            LoopAction::Stop => break,
        }
    }
}

// ── Backwards-compatible wrappers ────────────────────────────────────────────

/// Run a supervised actor on Tokio.
///
/// Convenience wrapper around [`run`] with `RunConfig::supervised`.
pub async fn run_supervised_actor<S: MachineSpec + 'static>(
    machine: StateMachine<S>,
    domain_mailboxes: S::Mailboxes<TokioRuntime>,
    lifecycle_stream: TokioStream<LifecycleCommand>,
    actor_id: ActorId,
    supervisor_notify: TokioSender<ChildLifecycleEvent>,
) {
    let config = RunConfig::supervised(lifecycle_stream, actor_id, supervisor_notify);
    run(machine, domain_mailboxes, config, actor_id).await;
}

/// Run a supervised actor with abort mailbox support.
///
/// Convenience wrapper around [`run`] with `RunConfig::supervised_with_abort`.
pub async fn run_supervised_actor_with_abort<S: MachineSpec + 'static>(
    machine: StateMachine<S>,
    domain_mailboxes: S::Mailboxes<TokioRuntime>,
    lifecycle_stream: TokioStream<LifecycleCommand>,
    abort_stream: TokioStream<AbortCommand>,
    actor_id: ActorId,
    supervisor_notify: TokioSender<ChildLifecycleEvent>,
) {
    let config = RunConfig::supervised_with_abort(
        lifecycle_stream,
        abort_stream,
        actor_id,
        supervisor_notify,
    );
    run(machine, domain_mailboxes, config, actor_id).await;
}

// ── ChildGroupBuilder ─────────────────────────────────────────────────────────

pub use bloxide_child_management::ChildGroupBuilder as GenericChildGroupBuilder;

#[cfg(test)]
mod tests {
    use super::*;
    use bloxide_core::{
        capability::{BloxRuntime, DynamicChannelCap},
        event_tag::{EventTag, LifecycleEvent},
        mailboxes::NoMailboxes,
        spec::{MachineSpec, StateFns},
        topology::StateTopology,
    };
    use std::time::Duration;
    use tokio::time::sleep;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestState { Running }
    impl StateTopology for TestState {
        const STATE_COUNT: usize = 1;
        fn parent(self) -> Option<Self> { None }
        fn is_leaf(self) -> bool { true }
        fn path(self) -> &'static [Self] { match self { TestState::Running => &[TestState::Running] } }
        fn as_index(self) -> usize { match self { TestState::Running => 0 } }
    }
    #[derive(Clone, Copy)]
    struct TestEvent;
    impl EventTag for TestEvent { fn event_tag(&self) -> u8 { 0 } }
    impl LifecycleEvent for TestEvent { fn as_lifecycle_command(&self) -> Option<LifecycleCommand> { None } }
    struct TestSpec;
    const RUNNING_FNS: StateFns<TestSpec> = StateFns { on_entry: &[], on_exit: &[], transitions: &[] };
    impl MachineSpec for TestSpec {
        type State = TestState; type Event = TestEvent; type Ctx = ();
        type Mailboxes<R: BloxRuntime> = NoMailboxes;
        const HANDLER_TABLE: &'static [&'static StateFns<Self>] = &[&RUNNING_FNS];
        fn initial_state() -> Self::State { TestState::Running }
    }

    #[tokio::test]
    async fn report_outcome_logs_warning_when_channel_full() {
        let capacity: usize = 2;
        let id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (notify_ref, mut notify_rx) = <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(id, capacity);
        let notify = notify_ref.sender();
        let actor_id: ActorId = 42;
        for _ in 0..capacity {
            notify_ref.try_send(actor_id, ChildLifecycleEvent::Alive { child_id: actor_id }).expect("fill channel");
        }
        report_outcome::<TestSpec, TokioRuntime>(&DispatchOutcome::Failed, actor_id, &notify);
        let mut count = 0;
        let mut saw_failed = false;
        while let Ok(envelope) = notify_rx.inner.try_recv() {
            count += 1;
            if matches!(envelope.1, ChildLifecycleEvent::Failed { child_id: 42 }) { saw_failed = true; }
        }
        assert_eq!(count, capacity);
        assert!(!saw_failed, "Failed event should have been dropped");
    }

    #[tokio::test]
    async fn spawn_cap_kill_aborts_task() {
        use bloxide_spawn::SpawnCap;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let alive = Arc::new(AtomicBool::new(false));
        let alive_clone = alive.clone();
        let handle = <TokioRuntime as SpawnCap>::spawn(async move {
            alive_clone.store(true, Ordering::SeqCst);
            loop { sleep(Duration::from_secs(100)).await; }
        });
        sleep(Duration::from_millis(50)).await;
        assert!(alive.load(Ordering::SeqCst));
        let kill_handle = <TokioRuntime as SpawnCap>::kill_handle(handle);
        <TokioRuntime as SpawnCap>::kill(kill_handle);
        sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn ripcord_aborts_unresponsive_child() {
        use bloxide_child_management::{ChildGroup, ChildPolicy, GroupShutdown};
        use bloxide_spawn::SpawnCap;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let child_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (lifecycle_ref, _lifecycle_rx) = <TokioRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(child_id, 4);
        let (abort_ref, _abort_rx) = <TokioRuntime as DynamicChannelCap>::channel::<AbortCommand>(child_id, 4);
        let dropped = Arc::new(AtomicBool::new(false));
        let dropped_clone = dropped.clone();
        struct DropGuard(Arc<AtomicBool>);
        impl Drop for DropGuard { fn drop(&mut self) { self.0.store(true, Ordering::SeqCst); } }
        let handle = <TokioRuntime as SpawnCap>::spawn(async move {
            let _guard = DropGuard(dropped_clone);
            sleep(Duration::from_secs(100)).await;
        });
        sleep(Duration::from_millis(50)).await;
        assert!(!dropped.load(Ordering::SeqCst));
        let kill_handle = <TokioRuntime as SpawnCap>::kill_handle(handle);
        let mut group = ChildGroup::<TokioRuntime>::new(GroupShutdown::WhenAnyDone);
        group.add_dynamic(child_id, lifecycle_ref, abort_ref, kill_handle, ChildPolicy::Kill);
        let (notify_ref, _notify_rx) = <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(42, 16);
        group.handle_done_or_failed(child_id, 42, &notify_ref);
        sleep(Duration::from_millis(50)).await;
        assert!(dropped.load(Ordering::SeqCst), "task should have been killed by ripcord");
    }
}
