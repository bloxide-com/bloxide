// Copyright 2025 Bloxide, all rights reserved
//! Embassy runtime supervision support.
//!
//! The run loop itself lives in `bloxide_core::runloop`. The `ChildGroupBuilder`
//! is shared across all runtimes: it lives in `bloxide-child-management` and
//! reaches Embassy channels through the `GroupChannelCap` impl in `mailbox.rs`
//! (re-exported at the crate root). This module holds the supervision
//! integration tests.

#[cfg(all(test, feature = "std"))]
mod tests {
    use crate::EmbassyRuntime;
    use bloxide_core::lifecycle::ChildLifecycleEvent;
    use bloxide_core::{
        capability::{BloxRuntime, StaticChannelCap},
        engine::{DispatchOutcome, MachineState},
        event_tag::{EventTag, LifecycleEvent},
        lifecycle::LifecycleCommand,
        mailboxes::NoMailboxes,
        messaging::ActorId,
        report_outcome,
        spec::{MachineSpec, StateFns},
        topology::StateTopology,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestState {
        Running,
    }
    impl StateTopology for TestState {
        const STATE_COUNT: usize = 1;
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
            }
        }
        fn as_index(self) -> usize {
            match self {
                TestState::Running => 0,
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
    impl MachineSpec for TestSpec {
        type State = TestState;
        type Event = TestEvent;
        type Ctx = ();
        type Mailboxes<R: BloxRuntime> = NoMailboxes;
        const HANDLER_TABLE: &'static [&'static StateFns<Self>] = &[&RUNNING_FNS];
        fn initial_state() -> Self::State {
            TestState::Running
        }
    }

    #[test]
    fn started_reports_started_event() {
        let (notify_ref, notify_rx) =
            <EmbassyRuntime as StaticChannelCap>::channel::<ChildLifecycleEvent, 8>(999);
        let notify = notify_ref.sender();
        let actor_id: ActorId = 42;
        report_outcome::<TestSpec, EmbassyRuntime>(
            &DispatchOutcome::Started(MachineState::State(TestState::Running)),
            actor_id,
            &notify,
        );
        let first = notify_rx
            .inner
            .try_receive()
            .expect("expected one lifecycle event");
        assert!(matches!(
            first.1,
            ChildLifecycleEvent::Started { child_id: 42 }
        ));
        assert!(
            notify_rx.inner.try_receive().is_err(),
            "should be exactly one event"
        );
    }
}
