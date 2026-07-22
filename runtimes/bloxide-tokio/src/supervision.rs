// Copyright 2025 Bloxide, all rights reserved
//! Tokio runtime supervision support.
//!
//! The run loop itself lives in `bloxide-core::runloop`. This module provides
//! the Tokio-specific `ChildGroupBuilder` re-export and integration tests.

pub use bloxide_child_management::ChildGroupBuilder as GenericChildGroupBuilder;

#[cfg(test)]
mod tests {
    use bloxide_core::{
        capability::{BloxRuntime, DynamicChannelCap},
        engine::DispatchOutcome,
        lifecycle::{AbortCommand, ChildLifecycleEvent, LifecycleCommand},
        messaging::ActorId,
        report_outcome,
        spec::{MachineSpec, StateFns},
        topology::StateTopology,
        event_tag::{EventTag, LifecycleEvent},
        mailboxes::NoMailboxes,
    };
    use std::time::Duration;
    use tokio::time::sleep;

    use crate::TokioRuntime;

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
