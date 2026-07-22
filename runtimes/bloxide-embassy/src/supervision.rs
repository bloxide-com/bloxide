// Copyright 2025 Bloxide, all rights reserved
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

use crate::{EmbassyRuntime, EmbassySender, EmbassyStream};

// ── Unified run loop ─────────────────────────────────────────────────────────

/// Configuration for the unified [`run`] loop.
///
/// See [`bloxide_tokio::RunConfig`] for full documentation — the semantics are
/// identical, only the runtime types differ. Embassy has no abort stream
/// (NoKill), so `abort` is not present in this variant.
pub struct RunConfig {
    /// Lifecycle command stream from the supervisor. `None` for unsupervised/root actors.
    pub lifecycle: Option<EmbassyStream<LifecycleCommand>>,
    /// Sender to report `ChildLifecycleEvent` to the supervisor. `None` for unsupervised/root.
    pub supervisor_notify: Option<EmbassySender<ChildLifecycleEvent>>,
    /// Auto-start the actor before entering the loop. Use for unsupervised actors.
    pub auto_start: bool,
    /// Exit the loop when `DispatchOutcome::Stopped` is observed.
    /// `true` for root/unsupervised, `false` for supervised (stays alive in Init).
    pub exit_on_stop: bool,
}

impl RunConfig {
    /// Configuration for a root supervisor or root actor.
    pub fn root() -> Self {
        Self {
            lifecycle: None,
            supervisor_notify: None,
            auto_start: false,
            exit_on_stop: true,
        }
    }

    /// Configuration for a supervised child actor.
    pub fn supervised(
        lifecycle: EmbassyStream<LifecycleCommand>,
        supervisor_notify: EmbassySender<ChildLifecycleEvent>,
    ) -> Self {
        Self {
            lifecycle: Some(lifecycle),
            supervisor_notify: Some(supervisor_notify),
            auto_start: false,
            exit_on_stop: false,
        }
    }

    /// Configuration for an unsupervised actor that auto-starts and exits on stop.
    pub fn unsupervised() -> Self {
        Self {
            lifecycle: None,
            supervisor_notify: None,
            auto_start: true,
            exit_on_stop: true,
        }
    }
}

/// The unified run loop for Embassy-based actors.
///
/// Polls lifecycle → domain mailboxes in priority order, dispatches events
/// through the machine, and reports outcomes to the supervisor (if any).
/// Yields to the executor after each message to prevent task starvation.
///
/// The loop exits when:
/// - `exit_on_stop` is true and `DispatchOutcome::Stopped` is observed
/// - `DispatchOutcome::Aborted` is observed (always exits)
/// - `DispatchOutcome::Failed` is observed (always exits)
/// - Any polled stream returns `Poll::Ready(None)` (stream closed)
pub async fn run<S, M>(
    mut machine: StateMachine<S>,
    mut domain_mailboxes: M,
    config: RunConfig,
    actor_id: ActorId,
) where
    S: MachineSpec + 'static,
    M: Mailboxes<S::Event>,
{
    // Optional auto-start for unsupervised actors
    if config.auto_start {
        let outcome = machine.handle_lifecycle(LifecycleCommand::Start);
        if let Some(ref notify) = config.supervisor_notify {
            report_outcome::<S, EmbassyRuntime>(&outcome, actor_id, notify);
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
                            report_outcome::<S, EmbassyRuntime>(&outcome, actor_id, notify);
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

            // 2. Domain mailboxes
            match domain_mailboxes.poll_next(cx) {
                Poll::Ready(Some(event)) => {
                    let outcome = machine.dispatch(event);
                    if let Some(ref notify) = supervisor_notify {
                        report_outcome::<S, EmbassyRuntime>(&outcome, actor_id, notify);
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
                embassy_futures::yield_now().await;
            }
            LoopAction::Stop => break,
        }
    }
}

// ── Backwards-compatible wrapper ─────────────────────────────────────────────

/// Run a supervised actor on Embassy.
///
/// Convenience wrapper around [`run`] with [`RunConfig::supervised`].
pub async fn run_supervised_actor<S: MachineSpec + 'static>(
    machine: StateMachine<S>,
    domain_mailboxes: S::Mailboxes<EmbassyRuntime>,
    lifecycle_stream: EmbassyStream<LifecycleCommand>,
    actor_id: ActorId,
    supervisor_notify: EmbassySender<ChildLifecycleEvent>,
) {
    let config = RunConfig::supervised(lifecycle_stream, supervisor_notify);
    run(machine, domain_mailboxes, config, actor_id).await;
}

// ── ChildGroupBuilder ─────────────────────────────────────────────────────────
//
// Static-channel builder for Embassy. Generic over the control message type `Ctrl`
// — the runtime does NOT know about `SupervisorControl`. The app chooses `Ctrl`.

pub struct ChildGroupBuilder<Ctrl: Send + 'static> {
    group: bloxide_child_management::ChildGroup<EmbassyRuntime>,
    notify_ref: bloxide_core::messaging::ActorRef<ChildLifecycleEvent, EmbassyRuntime>,
    notify_rx: EmbassyStream<ChildLifecycleEvent>,
    control_ref: bloxide_core::messaging::ActorRef<Ctrl, EmbassyRuntime>,
    control_rx: EmbassyStream<Ctrl>,
}

impl<Ctrl: Send + 'static> ChildGroupBuilder<Ctrl> {
    pub fn new(shutdown: bloxide_child_management::GroupShutdown) -> Self {
        let (notify_ref, notify_rx) =
            <EmbassyRuntime as bloxide_core::capability::StaticChannelCap>::channel::<
                ChildLifecycleEvent,
                32,
            >(bloxide_macros::next_actor_id!());
        let (control_ref, control_rx) =
            <EmbassyRuntime as bloxide_core::capability::StaticChannelCap>::channel::<Ctrl, 16>(
                bloxide_macros::next_actor_id!(),
            );
        Self {
            group: bloxide_child_management::ChildGroup::new(shutdown),
            notify_ref,
            notify_rx,
            control_ref,
            control_rx,
        }
    }

    pub fn add_child(
        &mut self,
        id: ActorId,
        policy: bloxide_child_management::ChildPolicy,
    ) -> (
        EmbassyStream<LifecycleCommand>,
        EmbassySender<ChildLifecycleEvent>,
    ) {
        let (lifecycle_ref, cmd_rx) =
            <EmbassyRuntime as bloxide_core::capability::StaticChannelCap>::channel::<
                LifecycleCommand,
                4,
            >(id);
        self.group.add(id, lifecycle_ref, policy);
        (cmd_rx, self.notify_ref.sender())
    }

    pub fn control_ref(&self) -> bloxide_core::messaging::ActorRef<Ctrl, EmbassyRuntime> {
        self.control_ref.clone()
    }

    pub fn notify_sender(&self) -> EmbassySender<ChildLifecycleEvent> {
        self.notify_ref.sender()
    }

    pub fn notify_ref(
        &self,
    ) -> bloxide_core::messaging::ActorRef<ChildLifecycleEvent, EmbassyRuntime> {
        self.notify_ref.clone()
    }

    pub fn finish(
        self,
    ) -> (
        bloxide_child_management::ChildGroup<EmbassyRuntime>,
        EmbassyStream<ChildLifecycleEvent>,
        EmbassyStream<Ctrl>,
    ) {
        (self.group, self.notify_rx, self.control_rx)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::report_outcome;
    use crate::EmbassyRuntime;
    use bloxide_core::lifecycle::ChildLifecycleEvent;
    use bloxide_core::{
        capability::{BloxRuntime, StaticChannelCap},
        engine::{DispatchOutcome, MachineState},
        event_tag::{EventTag, LifecycleEvent},
        lifecycle::LifecycleCommand,
        mailboxes::NoMailboxes,
        messaging::ActorId,
        spec::{MachineSpec, StateFns},
        topology::StateTopology,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestState { Running }
    impl StateTopology for TestState {
        const STATE_COUNT: usize = 1;
        fn parent(self) -> Option<Self> { let _ = self; None }
        fn is_leaf(self) -> bool { let _ = self; true }
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

    #[test]
    fn started_reports_started_event() {
        let (notify_ref, notify_rx) = <EmbassyRuntime as StaticChannelCap>::channel::<ChildLifecycleEvent, 8>(999);
        let notify = notify_ref.sender();
        let actor_id: ActorId = 42;
        report_outcome::<TestSpec, EmbassyRuntime>(
            &DispatchOutcome::Started(MachineState::State(TestState::Running)), actor_id, &notify,
        );
        let first = notify_rx.inner.try_receive().expect("expected one lifecycle event");
        assert!(matches!(first.1, ChildLifecycleEvent::Started { child_id: 42 }));
        assert!(notify_rx.inner.try_receive().is_err(), "should be exactly one event");
    }
}
