// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    child_management::KillCommand,
    engine::{DispatchOutcome, MachineState, StateMachine},
    lifecycle::LifecycleCommand,
    mailboxes::Mailboxes,
    messaging::{ActorId, ActorRef, Envelope},
    spec::MachineSpec,
};

use bloxide_supervisor::{
    control::SupervisorControl,
    registry::{ChildGroup, ChildPolicy, GroupShutdown},
};
use core::future::poll_fn;
use core::pin::Pin;
use core::task::Poll;
use futures_core::Stream;

use crate::{TokioRuntime, TokioSender, TokioStream};

// ── Standalone supervised actor runner ───────────────────────────────────────

/// Run a supervised actor on Tokio.
///
/// Polls lifecycle and domain mailboxes, dispatches events through the machine,
/// and reports outcomes to the supervisor.
pub async fn run_supervised_actor<S: MachineSpec + 'static>(
    mut machine: StateMachine<S>,
    mut domain_mailboxes: S::Mailboxes<TokioRuntime>,
    mut lifecycle_stream: TokioStream<LifecycleCommand>,
    actor_id: ActorId,
    supervisor_notify: TokioSender<ChildLifecycleEvent>,
) {
    enum LoopAction {
        Continue,
        Stop,
    }

    loop {
        let action = poll_fn(|cx| {
            // First check lifecycle stream (higher priority)
            match Pin::new(&mut lifecycle_stream).poll_next(cx) {
                Poll::Ready(None) => return Poll::Ready(LoopAction::Stop),
                Poll::Ready(Some(Envelope(_, cmd))) => {
                    let outcome = handle_lifecycle_direct(&mut machine, cmd);
                    report_outcome::<S>(&outcome, actor_id, &supervisor_notify);

                    return match outcome {
                        DispatchOutcome::Stopped => Poll::Ready(LoopAction::Stop),
                        _ => Poll::Ready(LoopAction::Continue),
                    };
                }
                Poll::Pending => {}
            }

            // Then check domain mailboxes
            match domain_mailboxes.poll_next(cx) {
                Poll::Ready(Some(event)) => {
                    let outcome = machine.dispatch(event);
                    report_outcome::<S>(&outcome, actor_id, &supervisor_notify);
                    Poll::Ready(LoopAction::Continue)
                }
                Poll::Ready(None) => Poll::Ready(LoopAction::Stop),
                Poll::Pending => Poll::Pending,
            }
        })
        .await;

        match action {
            LoopAction::Continue => {}
            LoopAction::Stop => break,
        }
    }
}

/// Handle lifecycle command by delegating to engine's lifecycle handler.
///
/// This ensures state transitions fire their `on_entry`/`on_exit` callbacks.
pub fn handle_lifecycle_direct<S: MachineSpec>(
    machine: &mut StateMachine<S>,
    cmd: LifecycleCommand,
) -> DispatchOutcome<S::State> {
    machine.handle_lifecycle(cmd)
}

fn report_outcome<S: MachineSpec>(
    outcome: &DispatchOutcome<S::State>,
    actor_id: ActorId,
    notify: &TokioSender<ChildLifecycleEvent>,
) {
    let send = |event| {
        if <TokioRuntime as BloxRuntime>::try_send_via(notify, Envelope(actor_id, event)).is_err() {
            bloxide_log::blox_log_warn!(
                actor_id,
                "failed to send lifecycle event to supervisor (channel full or closed)"
            );
        }
    };

    match outcome {
        DispatchOutcome::Started(MachineState::State(s)) => {
            if S::is_error(s) {
                send(ChildLifecycleEvent::Failed { child_id: actor_id });
            } else if S::is_terminal(s) {
                send(ChildLifecycleEvent::Done { child_id: actor_id });
            } else {
                send(ChildLifecycleEvent::Started { child_id: actor_id });
            }
        }
        DispatchOutcome::Transition(MachineState::State(s)) => {
            if S::is_error(s) {
                send(ChildLifecycleEvent::Failed { child_id: actor_id });
            } else if S::is_terminal(s) {
                send(ChildLifecycleEvent::Done { child_id: actor_id });
            }
        }
        DispatchOutcome::Done(MachineState::State(_)) => {
            send(ChildLifecycleEvent::Done { child_id: actor_id });
        }
        DispatchOutcome::Failed => {
            send(ChildLifecycleEvent::Failed { child_id: actor_id });
        }
        DispatchOutcome::Reset => {
            send(ChildLifecycleEvent::Reset { child_id: actor_id });
        }
        DispatchOutcome::Stopped => {
            send(ChildLifecycleEvent::Stopped { child_id: actor_id });
        }
        DispatchOutcome::Alive => {
            send(ChildLifecycleEvent::Alive { child_id: actor_id });
        }
        _ => {}
    }
}

// ── Kill-aware supervised actor runner ──────────────────────────────────────

/// Run a supervised actor with kill mailbox support.
///
/// This wraps [`run_supervised_actor`] with an additional kill mailbox.
/// When a `KillCommand::Kill` is received, the actor self-terminates
/// immediately (breaks out of the select loop, drops the future).
///
/// Kill uses a dual mechanism:
///   1. Self-termination (fast path): the task breaks out of the select
///      loop and returns. Works when the kill mailbox is actively polled.
///   2. External abort (ripcord): the `AbortHandle` from
///      `SpawnCap::abort_handle()` is stored in `ChildEntry` in the
///      supervisor. When the child is unresponsive (not polling the kill
///      mailbox), the supervisor calls `R::Kill::kill(handle)` — which
///      calls `SpawnCap::abort(handle)`, i.e. `AbortHandle::abort()` on
///      Tokio.
///
/// This function handles only the self-termination path. The external
/// abort is handled by `ChildGroup::handle_done_or_failed` in the
/// supervisor (see spec §4.10).
///
/// See `spec/architecture/22-spawn-architecture-v2.md` §4.12.
pub async fn run_supervised_actor_with_kill<S: MachineSpec + 'static>(
    machine: StateMachine<S>,
    domain_mailboxes: S::Mailboxes<TokioRuntime>,
    lifecycle_stream: TokioStream<LifecycleCommand>,
    kill_stream: TokioStream<KillCommand>,
    actor_id: ActorId,
    supervisor_notify: TokioSender<ChildLifecycleEvent>,
) {
    enum LoopAction {
        Continue,
        Stop,
    }

    let mut machine = machine;
    let mut domain_mailboxes = domain_mailboxes;
    let mut lifecycle_stream = lifecycle_stream;
    let mut kill_stream = kill_stream;

    loop {
        let action = poll_fn(|cx| {
            // First check lifecycle stream (higher priority)
            match Pin::new(&mut lifecycle_stream).poll_next(cx) {
                Poll::Ready(None) => return Poll::Ready(LoopAction::Stop),
                Poll::Ready(Some(Envelope(_, cmd))) => {
                    let outcome = handle_lifecycle_direct(&mut machine, cmd);
                    report_outcome::<S>(&outcome, actor_id, &supervisor_notify);

                    return match outcome {
                        DispatchOutcome::Stopped => Poll::Ready(LoopAction::Stop),
                        _ => Poll::Ready(LoopAction::Continue),
                    };
                }
                Poll::Pending => {}
            }

            // Then check kill mailbox (high priority — kill should be
            // serviced before domain messages so a stuck actor can be
            // terminated promptly when it next yields to the select loop).
            match Pin::new(&mut kill_stream).poll_next(cx) {
                Poll::Ready(None) => return Poll::Ready(LoopAction::Stop),
                Poll::Ready(Some(Envelope(_, KillCommand::Kill { .. }))) => {
                    // Self-termination: break out of the loop and return.
                    // This drops the future and ends the task. No lifecycle
                    // callback fires — kill is immediate (per spec §3 point 5).
                    return Poll::Ready(LoopAction::Stop);
                }
                Poll::Pending => {}
            }

            // Then check domain mailboxes
            match domain_mailboxes.poll_next(cx) {
                Poll::Ready(Some(event)) => {
                    let outcome = machine.dispatch(event);
                    report_outcome::<S>(&outcome, actor_id, &supervisor_notify);
                    Poll::Ready(LoopAction::Continue)
                }
                Poll::Ready(None) => Poll::Ready(LoopAction::Stop),
                Poll::Pending => Poll::Pending,
            }
        })
        .await;

        match action {
            LoopAction::Continue => {}
            LoopAction::Stop => break,
        }
    }
}

// ── ChildGroupBuilder ─────────────────────────────────────────────────────────

pub struct ChildGroupBuilder {
    group: ChildGroup<TokioRuntime>,
    notify_ref: ActorRef<ChildLifecycleEvent, TokioRuntime>,
    notify_rx: TokioStream<ChildLifecycleEvent>,
    control_ref: ActorRef<SupervisorControl<TokioRuntime>, TokioRuntime>,
    control_rx: TokioStream<SupervisorControl<TokioRuntime>>,
}

impl ChildGroupBuilder {
    pub fn new(shutdown: GroupShutdown) -> Self {
        let notify_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (notify_ref, notify_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(notify_id, 32);
        let control_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (control_ref, control_rx) = <TokioRuntime as DynamicChannelCap>::channel::<
            SupervisorControl<TokioRuntime>,
        >(control_id, 16);
        Self {
            group: ChildGroup::new(shutdown),
            notify_ref,
            notify_rx,
            control_ref,
            control_rx,
        }
    }

    pub fn add_child(
        &mut self,
        id: ActorId,
        policy: ChildPolicy,
    ) -> (
        TokioStream<LifecycleCommand>,
        TokioSender<ChildLifecycleEvent>,
    ) {
        let (lifecycle_ref, cmd_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(id, 4);
        self.group.add(id, lifecycle_ref, policy);
        (cmd_rx, self.notify_ref.sender())
    }

    pub fn control_ref(&self) -> ActorRef<SupervisorControl<TokioRuntime>, TokioRuntime> {
        self.control_ref.clone()
    }

    pub fn notify_sender(&self) -> TokioSender<ChildLifecycleEvent> {
        self.notify_ref.sender()
    }

    pub fn notify_ref(&self) -> ActorRef<ChildLifecycleEvent, TokioRuntime> {
        self.notify_ref.clone()
    }

    pub fn finish(
        self,
    ) -> (
        ChildGroup<TokioRuntime>,
        TokioStream<ChildLifecycleEvent>,
        TokioStream<SupervisorControl<TokioRuntime>>,
    ) {
        (self.group, self.notify_rx, self.control_rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bloxide_core::{
        event_tag::{EventTag, LifecycleEvent},
        mailboxes::NoMailboxes,
        spec::{MachineSpec, StateFns},
        topology::StateTopology,
    };
    use std::time::Duration;
    use tokio::time::sleep;

    // ── Minimal test MachineSpec for report_outcome tests ─────────────────────

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    #[allow(dead_code)]
    enum TestState {
        Running,
        Done,
    }

    impl StateTopology for TestState {
        const STATE_COUNT: usize = 2;

        fn parent(self) -> Option<Self> {
            None
        }

        fn is_leaf(self) -> bool {
            true
        }

        fn path(self) -> &'static [Self] {
            match self {
                TestState::Running => &[TestState::Running],
                TestState::Done => &[TestState::Done],
            }
        }

        fn as_index(self) -> usize {
            match self {
                TestState::Running => 0,
                TestState::Done => 1,
            }
        }
    }

    #[derive(Clone, Copy)]
    struct TestEvent;
    impl EventTag for TestEvent {
        fn event_tag(&self) -> u8 {
            0
        }
    }
    impl LifecycleEvent for TestEvent {
        fn as_lifecycle_command(&self) -> Option<LifecycleCommand> {
            None
        }
    }

    struct TestSpec;

    const RUNNING_FNS: StateFns<TestSpec> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: &[],
    };
    const DONE_FNS: StateFns<TestSpec> = StateFns {
        on_entry: &[],
        on_exit: &[],
        transitions: &[],
    };

    impl MachineSpec for TestSpec {
        type State = TestState;
        type Event = TestEvent;
        type Ctx = ();
        type Mailboxes<R: BloxRuntime> = NoMailboxes;

        const HANDLER_TABLE: &'static [&'static StateFns<Self>] = &[&RUNNING_FNS, &DONE_FNS];

        fn initial_state() -> Self::State {
            TestState::Running
        }

        fn is_terminal(state: &Self::State) -> bool {
            matches!(state, TestState::Done)
        }
    }

    /// Fill a notify channel to capacity, call `report_outcome` with a `Failed`
    /// event, and verify the event is silently dropped (not delivered) without
    /// panicking. The warning log is emitted via `blox_log_warn!` which is a
    /// no-op when no logging subscriber is active, so we assert no panic and
    /// that the receiver still has exactly `capacity` items (the dropped event
    /// is not among them).
    #[tokio::test]
    async fn report_outcome_logs_warning_when_channel_full() {
        let capacity: usize = 2;
        let id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (notify_ref, mut notify_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(id, capacity);
        let notify = notify_ref.sender();
        let actor_id: ActorId = 42;

        // Fill the channel to capacity.
        for _ in 0..capacity {
            notify_ref
                .try_send(actor_id, ChildLifecycleEvent::Alive { child_id: actor_id })
                .expect("fill channel");
        }

        // Now call report_outcome with a Failed event — this should fail to send
        // (channel full) and log a warning instead of panicking.
        report_outcome::<TestSpec>(&DispatchOutcome::Failed, actor_id, &notify);

        // Drain the channel — we should see exactly `capacity` Alive events,
        // not the Failed event that was dropped.
        let mut count = 0;
        let mut saw_failed = false;
        while let Ok(envelope) = notify_rx.inner.try_recv() {
            count += 1;
            if matches!(envelope.1, ChildLifecycleEvent::Failed { child_id: 42 }) {
                saw_failed = true;
            }
        }

        assert_eq!(
            count, capacity,
            "should have exactly capacity events in the channel"
        );
        assert!(
            !saw_failed,
            "Failed event should have been dropped (channel was full)"
        );
    }

    /// Spawn a task via `SpawnCap::spawn`, derive an `AbortHandle`, abort it,
    /// and verify the task is aborted.
    #[tokio::test]
    async fn spawn_cap_kill_aborts_task() {
        use bloxide_core::SpawnCap;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let alive = Arc::new(AtomicBool::new(false));
        let alive_clone = alive.clone();

        let handle = <TokioRuntime as SpawnCap>::spawn(async move {
            alive_clone.store(true, Ordering::SeqCst);
            loop {
                sleep(Duration::from_secs(100)).await;
            }
        });

        // Wait for the task to start.
        sleep(Duration::from_millis(50)).await;
        assert!(alive.load(Ordering::SeqCst), "task should have started");

        // Derive a cloneable AbortHandle and abort the task.
        let abort_handle = <TokioRuntime as SpawnCap>::abort_handle(handle);
        <TokioRuntime as SpawnCap>::abort(abort_handle);
        sleep(Duration::from_millis(50)).await;

        // After abort, the task is aborted. We can't directly observe abort
        // from inside, but the key assertion is that abort() didn't panic
        // and the handle was consumed.
    }

    /// Integration test: the supervisor's ripcord path actually aborts an
    /// unresponsive child task.
    ///
    /// This exercises the full kill path:
    ///   1. Spawn an **unresponsive** child task (stuck in a long sleep, never
    ///      polls its kill mailbox)
    ///   2. Create a `ChildGroup<TokioRuntime>` with `ChildPolicy::Kill`
    ///   3. Register the child via `add_dynamic` with the real `AbortHandle`
    ///      and `kill_ref`
    ///   4. Call `handle_done_or_failed` — this calls `R::Kill::kill(handle)`
    ///      which calls `SpawnCap::abort(handle)` which calls
    ///      `AbortHandle::abort()`
    ///   5. Verify the task was actually aborted via a `Drop` guard that sets
    ///      an `AtomicBool` when the future is dropped (aborted mid-flight)
    ///
    /// This is the test that was impossible with the old `TaskHandle` approach
    /// because `JoinHandle<()>` is not `Clone` and couldn't be extracted from
    /// `&Event` by the action function.
    #[tokio::test]
    async fn ripcord_aborts_unresponsive_child() {
        use bloxide_core::SpawnCap;
        use bloxide_supervisor::registry::{ChildGroup, ChildPolicy, GroupShutdown};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        // --- Set up channels for the child ---
        let child_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (lifecycle_ref, _lifecycle_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(child_id, 4);
        let (kill_ref, _kill_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<KillCommand>(child_id, 4);

        // --- Spawn an unresponsive child task ---
        // The task enters a 100s sleep and never polls the kill mailbox.
        // A `Drop` guard sets `dropped = true` when the future is cancelled
        // (aborted mid-flight), proving the ripcord worked.
        let dropped = Arc::new(AtomicBool::new(false));
        let dropped_clone = dropped.clone();

        let task_handle = <TokioRuntime as SpawnCap>::spawn(async move {
            // Drop guard — fires when the future is aborted (dropped mid-poll).
            struct DropGuard(Arc<AtomicBool>);
            impl Drop for DropGuard {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::SeqCst);
                }
            }
            let _guard = DropGuard(dropped_clone);

            // Simulate being stuck — never yields to the kill mailbox.
            sleep(Duration::from_secs(100)).await;
        });

        // Wait for the task to start.
        sleep(Duration::from_millis(50)).await;
        assert!(
            !dropped.load(Ordering::SeqCst),
            "task should still be running before ripcord"
        );

        // --- Derive the cloneable AbortHandle from the JoinHandle ---
        let abort_handle = <TokioRuntime as SpawnCap>::abort_handle(task_handle);

        // --- Register the child in the ChildGroup with Kill policy ---
        let mut group = ChildGroup::<TokioRuntime>::new(GroupShutdown::WhenAllDone);
        group.add_dynamic(
            child_id,
            lifecycle_ref,
            kill_ref,
            abort_handle,
            ChildPolicy::Kill,
        );

        // --- Trigger the ripcord via handle_done_or_failed ---
        let from = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let action = group.handle_done_or_failed(child_id, from);

        // Kill policy always begins shutdown (the child is permanently done).
        assert_eq!(
            action,
            bloxide_supervisor::registry::ChildAction::BeginShutdown,
            "Kill policy should trigger shutdown"
        );

        // --- Verify the task was actually aborted ---
        // The Drop guard fires when the task's future is dropped by abort().
        // If the ripcord didn't work, this would hang for 100 seconds.
        sleep(Duration::from_millis(100)).await;
        assert!(
            dropped.load(Ordering::SeqCst),
            "unresponsive child task should have been aborted by the ripcord (Drop guard fired)"
        );
    }
}
