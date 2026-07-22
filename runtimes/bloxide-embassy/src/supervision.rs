// Copyright 2025 Bloxide, all rights reserved
//! Embassy runtime supervision support.
//!
//! The run loop itself lives in `bloxide_core::runloop`. This module provides
//! the Embassy-specific `ChildGroupBuilder` and integration tests.

use bloxide_core::{
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::ActorId,
};

use crate::{EmbassyRuntime, EmbassySender, EmbassyStream};

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
    use bloxide_core::lifecycle::ChildLifecycleEvent;
    use crate::EmbassyRuntime;
    use bloxide_core::{
        capability::{BloxRuntime, StaticChannelCap},
        engine::{DispatchOutcome, MachineState},
        event_tag::{EventTag, LifecycleEvent},
        lifecycle::LifecycleCommand,
        mailboxes::NoMailboxes,
        messaging::ActorId,
        spec::{MachineSpec, StateFns},
        topology::StateTopology,
        report_outcome,
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
