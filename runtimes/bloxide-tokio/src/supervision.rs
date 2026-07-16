// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    engine::{DispatchOutcome, MachineState, StateMachine},
    lifecycle::LifecycleCommand,
    mailboxes::Mailboxes,
    messaging::{ActorId, ActorRef, Envelope},
    spec::MachineSpec,
};

use bloxide_supervisor::{
    control::RegisterChild,
    control::SupervisorControl,
    registry::{ChildGroup, ChildPolicy, GroupShutdown},
};
use core::future::{poll_fn, Future};
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

// ── ChildGroupBuilder ─────────────────────────────────────────────────────────

pub struct ChildGroupBuilder {
    group: ChildGroup<TokioRuntime>,
    notify_tx: TokioSender<ChildLifecycleEvent>,
    notify_rx: TokioStream<ChildLifecycleEvent>,
    control_ref: ActorRef<SupervisorControl<TokioRuntime>, TokioRuntime>,
    control_rx: TokioStream<SupervisorControl<TokioRuntime>>,
    /// KillCap for policy-driven cleanup of dynamic actors.
    kill_cap: std::sync::Arc<crate::TokioKillCap>,
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
        let kill_cap = std::sync::Arc::new(crate::TokioKillCap::new());
        Self {
            group: ChildGroup::with_kill_cap(shutdown, kill_cap.clone()),
            notify_tx: notify_ref.sender(),
            notify_rx,
            control_ref,
            control_rx,
            kill_cap,
        }
    }

    /// Create a new ChildGroupBuilder with a custom KillCap for dynamic actor cleanup.
    pub fn with_kill_cap(
        shutdown: GroupShutdown,
        kill_cap: std::sync::Arc<crate::TokioKillCap>,
    ) -> Self {
        let notify_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (notify_ref, notify_rx) =
            <TokioRuntime as DynamicChannelCap>::channel::<ChildLifecycleEvent>(notify_id, 32);
        let control_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
        let (control_ref, control_rx) = <TokioRuntime as DynamicChannelCap>::channel::<
            SupervisorControl<TokioRuntime>,
        >(control_id, 16);
        Self {
            group: ChildGroup::with_kill_cap(shutdown, kill_cap.clone()),
            notify_tx: notify_ref.sender(),
            notify_rx,
            control_ref,
            control_rx,
            kill_cap,
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
        (cmd_rx, self.notify_tx.clone())
    }

    /// Returns a reference to the KillCap for registering spawned tasks.
    pub fn kill_cap(&self) -> &std::sync::Arc<crate::TokioKillCap> {
        &self.kill_cap
    }

    pub fn control_ref(&self) -> ActorRef<SupervisorControl<TokioRuntime>, TokioRuntime> {
        self.control_ref.clone()
    }

    pub fn notify_sender(&self) -> TokioSender<ChildLifecycleEvent> {
        self.notify_tx.clone()
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

/// Spawn a dynamic supervised child actor and register its `JoinHandle` with `kill_cap`.
///
/// Registration is required so that `kill_cap.kill(child_id)` can abort the task.
pub fn spawn_dynamic_supervised_child<F, Fut>(
    from: ActorId,
    control_ref: &ActorRef<SupervisorControl<TokioRuntime>, TokioRuntime>,
    notify_sender: &TokioSender<ChildLifecycleEvent>,
    kill_cap: &crate::TokioKillCap,
    child_id: ActorId,
    policy: ChildPolicy,
    task_builder: F,
) -> Result<(), <TokioRuntime as BloxRuntime>::TrySendError>
where
    F: FnOnce(TokioStream<LifecycleCommand>, TokioSender<ChildLifecycleEvent>, ActorId) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (lifecycle_ref, lifecycle_rx) =
        <TokioRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(child_id, 4);

    control_ref.try_send(
        from,
        SupervisorControl::RegisterChild(RegisterChild {
            id: child_id,
            lifecycle_ref,
            policy,
        }),
    )?;

    let notify = notify_sender.clone();
    let handle = tokio::spawn(task_builder(lifecycle_rx, notify, child_id));
    kill_cap.register(child_id, handle);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bloxide_core::KillCap;
    use bloxide_core::{
        event_tag::{EventTag, LifecycleEvent},
        mailboxes::NoMailboxes,
        spec::{MachineSpec, StateFns},
        topology::StateTopology,
    };
    use bloxide_supervisor::registry::GroupShutdown;
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
            if matches!(
                envelope.1,
                ChildLifecycleEvent::Failed { child_id: 42 }
            ) {
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

    /// Spawn a dynamic child, call `kill_cap.kill(child_id)`, and verify the
    /// underlying task is aborted — the entry is removed from the KillCap.
    #[tokio::test]
    async fn spawn_dynamic_child_then_kill_aborts_task() {
        let group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
        let control_ref = group.control_ref();
        let notify = group.notify_sender();
        let kill_cap = group.kill_cap().clone();
        let child_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();

        let (children, _notify_rx, _control_rx) = group.finish();
        // Hold children so the control channel stays alive.
        let _children = children;

        spawn_dynamic_supervised_child(
            child_id,
            &control_ref,
            &notify,
            &kill_cap,
            child_id,
            ChildPolicy::Kill,
            |_lc_rx, _sup_notify, _actor_id| async move {
                loop {
                    sleep(Duration::from_secs(100)).await;
                }
            },
        )
        .expect("register dynamic child");

        assert!(kill_cap.contains(child_id));
        kill_cap.kill(child_id);
        sleep(Duration::from_millis(50)).await;
        assert!(!kill_cap.contains(child_id));
    }

    /// Spawn a dynamic child and verify it is registered with the KillCap
    /// (the child ID appears in the KillCap's internal task map).
    #[tokio::test]
    async fn spawn_dynamic_child_registers_with_kill_cap() {
        let group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
        let control_ref = group.control_ref();
        let notify = group.notify_sender();
        let kill_cap = group.kill_cap().clone();
        let child_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();

        let (children, _notify_rx, _control_rx) = group.finish();
        let _children = children;

        spawn_dynamic_supervised_child(
            child_id,
            &control_ref,
            &notify,
            &kill_cap,
            child_id,
            ChildPolicy::Kill,
            |_lc_rx, _sup_notify, _actor_id| async move {
                loop {
                    sleep(Duration::from_secs(100)).await;
                }
            },
        )
        .expect("register dynamic child");

        assert!(kill_cap.contains(child_id));
        kill_cap.kill(child_id);
    }
}
