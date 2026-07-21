// Copyright 2025 Bloxide, all rights reserved
use bloxide_child_management::{ChildGroup, ChildPolicy, GroupShutdown};
use bloxide_core::{
    capability::StaticChannelCap,
    engine::{DispatchOutcome, StateMachine},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    mailboxes::Mailboxes,
    messaging::{ActorId, ActorRef, Envelope},
    spec::MachineSpec,
    report_outcome,
};
use core::future::poll_fn;
use core::pin::Pin;
use core::task::Poll;
use futures_core::Stream;

use crate::{EmbassyRuntime, EmbassySender, EmbassyStream};

// ── Standalone supervised actor runner ───────────────────────────────────────

/// Run a supervised actor on Embassy.
///
/// Polls lifecycle and domain mailboxes, dispatches events through the machine,
/// and reports outcomes to the supervisor.
pub async fn run_supervised_actor<S: MachineSpec + 'static>(
    mut machine: StateMachine<S>,
    mut domain_mailboxes: S::Mailboxes<EmbassyRuntime>,
    mut lifecycle_stream: EmbassyStream<LifecycleCommand>,
    actor_id: ActorId,
    supervisor_notify: EmbassySender<ChildLifecycleEvent>,
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
                    let outcome = handle_lifecycle_via_dispatch(&mut machine, cmd);
                    report_outcome::<S, EmbassyRuntime>(&outcome, actor_id, &supervisor_notify);

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
                    report_outcome::<S, EmbassyRuntime>(&outcome, actor_id, &supervisor_notify);
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
fn handle_lifecycle_via_dispatch<S: MachineSpec>(
    machine: &mut StateMachine<S>,
    cmd: LifecycleCommand,
) -> DispatchOutcome<S::State> {
    machine.handle_lifecycle(cmd)
}

// ── ChildGroupBuilder ─────────────────────────────────────────────────────────
//
// Static-channel builder for Embassy. Generic over the control message type `Ctrl`
// — the runtime does NOT know about `SupervisorControl`. The app chooses `Ctrl`.

pub struct ChildGroupBuilder<Ctrl: Send + 'static> {
    group: ChildGroup<EmbassyRuntime>,
    notify_ref: ActorRef<ChildLifecycleEvent, EmbassyRuntime>,
    notify_rx: EmbassyStream<ChildLifecycleEvent>,
    control_ref: ActorRef<Ctrl, EmbassyRuntime>,
    control_rx: EmbassyStream<Ctrl>,
}

impl<Ctrl: Send + 'static> ChildGroupBuilder<Ctrl> {
    pub fn new(shutdown: GroupShutdown) -> Self {
        let (notify_ref, notify_rx) = <EmbassyRuntime as StaticChannelCap>::channel::<
            ChildLifecycleEvent,
            32,
        >(bloxide_macros::next_actor_id!());
        let (control_ref, control_rx) = <EmbassyRuntime as StaticChannelCap>::channel::<Ctrl, 16>(
            bloxide_macros::next_actor_id!(),
        );
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
        EmbassyStream<LifecycleCommand>,
        EmbassySender<ChildLifecycleEvent>,
    ) {
        let (lifecycle_ref, cmd_rx) =
            <EmbassyRuntime as StaticChannelCap>::channel::<LifecycleCommand, 4>(id);
        self.group.add(id, lifecycle_ref, policy);
        (cmd_rx, self.notify_ref.sender())
    }

    pub fn control_ref(&self) -> ActorRef<Ctrl, EmbassyRuntime> {
        self.control_ref.clone()
    }

    pub fn notify_sender(&self) -> EmbassySender<ChildLifecycleEvent> {
        self.notify_ref.sender()
    }

    pub fn notify_ref(&self) -> ActorRef<ChildLifecycleEvent, EmbassyRuntime> {
        self.notify_ref.clone()
    }

    pub fn finish(
        self,
    ) -> (
        ChildGroup<EmbassyRuntime>,
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
    enum TestState {
        Running,
        Done,
    }

    impl StateTopology for TestState {
        const STATE_COUNT: usize = 2;

        fn parent(self) -> Option<Self> {
            let _ = self;
            None
        }

        fn is_leaf(self) -> bool {
            let _ = self;
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

    #[test]
    fn started_terminal_reports_done_only() {
        let (notify_ref, notify_rx) =
            <EmbassyRuntime as StaticChannelCap>::channel::<ChildLifecycleEvent, 8>(999);
        let notify = notify_ref.sender();
        let actor_id: ActorId = 42;

        report_outcome::<TestSpec, EmbassyRuntime>(
            &DispatchOutcome::Started(MachineState::State(TestState::Done)),
            actor_id,
            &notify,
        );

        let first = notify_rx
            .inner
            .try_receive()
            .expect("expected one lifecycle event");
        assert!(matches!(
            first.1,
            ChildLifecycleEvent::Done { child_id: 42 }
        ));
        assert!(
            notify_rx.inner.try_receive().is_err(),
            "terminal Started should not emit a second Started event"
        );
    }
}
