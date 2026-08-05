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
    use bloxide_child_management::{ChildGroup, ChildPolicy, GroupShutdown};
    use bloxide_core::lifecycle::ChildLifecycleEvent;
    use bloxide_core::{
        capability::{BloxRuntime, StaticChannelCap},
        engine::{DispatchOutcome, MachineState},
        event_tag::{EventTag, LifecycleEvent},
        lifecycle::LifecycleCommand,
        mailboxes::NoMailboxes,
        messaging::{ActorId, Envelope},
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

    #[test]
    fn try_send_error_is_closed_is_always_false_on_embassy() {
        let (notify_ref, notify_rx) =
            <EmbassyRuntime as StaticChannelCap>::channel::<ChildLifecycleEvent, 1>(7);

        // Full: capacity 1, the second send is genuine backpressure.
        notify_ref
            .try_send(1, ChildLifecycleEvent::Alive { child_id: 1 })
            .expect("first send fits");
        let err = notify_ref
            .try_send(1, ChildLifecycleEvent::Alive { child_id: 1 })
            .expect_err("second send is full");
        assert!(
            !EmbassyRuntime::try_send_error_is_closed(&err),
            "backpressure must not classify as closed"
        );

        // Embassy channels are process-lifetime statics with no Closed state:
        // once there is space again a send succeeds even after the receiver
        // handle is gone (EmbassyStream has no Drop glue — the channel is
        // unaffected). The Full-vs-Closed distinction the Tokio runtime tests
        // does not exist here — every failure is Full.
        let drained = {
            let rx = notify_rx;
            rx.inner.try_receive()
        }; // receiver handle ends here
        assert!(drained.is_ok(), "drain the first event to make space");
        assert!(
            notify_ref
                .try_send(1, ChildLifecycleEvent::Alive { child_id: 1 })
                .is_ok(),
            "embassy channels never close — send after receiver drop succeeds"
        );
    }

    #[test]
    fn report_outcome_drops_event_when_channel_full() {
        const CAPACITY: usize = 2;
        let (notify_ref, notify_rx) =
            <EmbassyRuntime as StaticChannelCap>::channel::<ChildLifecycleEvent, CAPACITY>(41);
        let notify = notify_ref.sender();
        let actor_id: ActorId = 42;
        for _ in 0..CAPACITY {
            notify_ref
                .try_send(actor_id, ChildLifecycleEvent::Alive { child_id: actor_id })
                .expect("fill channel");
        }
        report_outcome::<TestSpec, EmbassyRuntime>(&DispatchOutcome::Failed, actor_id, &notify);

        let mut count = 0;
        let mut saw_failed = false;
        while let Ok(envelope) = notify_rx.inner.try_receive() {
            count += 1;
            if matches!(envelope.1, ChildLifecycleEvent::Failed { child_id: 42 }) {
                saw_failed = true;
            }
        }
        assert_eq!(count, CAPACITY);
        assert!(!saw_failed, "Failed event should have been dropped");
    }

    /// The documented Embassy path for a dead child: Embassy has no kill and
    /// its channels never close, so a dead child is detected by health-check
    /// misses — `max_misses` consecutive unanswered watchdog Pings declare
    /// the child rogue and apply its `ChildPolicy`.
    #[test]
    fn watchdog_tick_detects_dead_child_via_health_misses() {
        let sup_id: ActorId = 40;
        let child_id: ActorId = 41;
        let (lifecycle_ref, lifecycle_rx) =
            <EmbassyRuntime as StaticChannelCap>::channel::<LifecycleCommand, 8>(child_id);
        let (notify_ref, _notify_rx) =
            <EmbassyRuntime as StaticChannelCap>::channel::<ChildLifecycleEvent, 16>(sup_id);

        let mut group = ChildGroup::<EmbassyRuntime>::new(GroupShutdown::WhenAnyDone, 2);
        group
            .try_add(child_id, lifecycle_ref, ChildPolicy::Reset { max: 3 })
            .expect("static child registration");
        group.handle_started(child_id, sup_id);

        // The child task is dead: nobody drains its lifecycle mailbox or
        // answers Ping with Alive.
        group.watchdog_tick(sup_id, &notify_ref); // Ping 1 delivered, outstanding
        group.watchdog_tick(sup_id, &notify_ref); // miss 1 (below max_misses), Ping 2
        group.watchdog_tick(sup_id, &notify_ref); // miss 2 -> rogue -> Reset policy

        let (mut pings, mut resets) = (0, 0);
        while let Ok(Envelope(_, cmd)) = lifecycle_rx.inner.try_receive() {
            match cmd {
                LifecycleCommand::Ping => pings += 1,
                LifecycleCommand::Reset => resets += 1,
                _ => {}
            }
        }
        assert_eq!(
            pings, 3,
            "one Ping per tick, including the ping to the ResetPending child"
        );
        assert_eq!(
            resets, 1,
            "two consecutive missed health checks must declare the child rogue \
             and apply its Reset policy"
        );
    }
}
