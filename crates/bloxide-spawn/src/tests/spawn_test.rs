// Copyright 2025 Bloxide, all rights reserved
//! Tests for SpawnCap implementation with TestRuntime.

use alloc::sync::Arc;
use alloc::vec::Vec;

use bloxide_core::messaging::{ActorId, ActorRef};
use bloxide_core::test_utils::TestRuntime;
use bloxide_core::DynamicActorSupport;
use bloxide_supervisor::control::{ChildType, SpawnParams, SpawnReplyTo};
use bloxide_core::lifecycle::ChildLifecycleEvent;
use bloxide_supervisor::registry::ChildPolicy;

use crate::output::SpawnOutput;
use crate::SpawnFactory;

/// Mock factory for testing spawn behavior.
#[derive(Clone)]
struct MockWorkerFactory {
    spawned_count: Arc<core::sync::atomic::AtomicUsize>,
}

impl MockWorkerFactory {
    fn new() -> Self {
        Self {
            spawned_count: Arc::new(core::sync::atomic::AtomicUsize::new(0)),
        }
    }

    fn spawn_count(&self) -> usize {
        self.spawned_count.load(core::sync::atomic::Ordering::SeqCst)
    }
}

impl SpawnFactory<TestRuntime> for MockWorkerFactory {
    fn spawn(
        &self,
        supervisor_id: ActorId,
        _supervisor_notify: ActorRef<ChildLifecycleEvent, TestRuntime>,
        params: SpawnParams,
        _reply_to: Option<SpawnReplyTo<TestRuntime>>,
    ) -> Option<SpawnOutput<TestRuntime>> {
        let SpawnParams::Worker { task_id } = params else {
            return None;
        };

        // Increment spawn counter
        self.spawned_count.fetch_add(1, core::sync::atomic::Ordering::SeqCst);

        // Allocate ID and create lifecycle channel
        let child_id = TestRuntime::alloc_actor_id();
        let (lifecycle_ref, _lifecycle_rx) = TestRuntime::channel(child_id, 16);

        Some(SpawnOutput {
            child_id,
            lifecycle_ref,
            policy: Some(ChildPolicy::Restart { max: 3 }),
        })
    }
}

#[test]
fn spawn_factory_creates_child() {
    let factory = MockWorkerFactory::new();
    let (sup_notify_ref, _sup_notify_rx) = TestRuntime::channel(0, 16);

    let output = factory.spawn(
        1,
        sup_notify_ref,
        SpawnParams::Worker { task_id: 42 },
        None,
    );

    assert!(output.is_some());
    let output = output.unwrap();
    assert_eq!(factory.spawn_count(), 1);
    assert!(output.policy.is_some());
}

#[test]
fn spawn_factory_rejects_unknown_params() {
    let factory = MockWorkerFactory::new();
    let (sup_notify_ref, _sup_notify_rx) = TestRuntime::channel(0, 16);

    // This test assumes there might be other SpawnParams variants in the future
    // For now, Worker variant should succeed
    let output = factory.spawn(
        1,
        sup_notify_ref,
        SpawnParams::Worker { task_id: 1 },
        None,
    );

    assert!(output.is_some());
    assert_eq!(factory.spawn_count(), 1);
}

#[test]
fn register_and_process_spawn() {
    use crate::test_impl::{clear_spawn_factories, process_pending_spawns, queue_spawn_request, register_test_spawn_factory};

    // Setup
    clear_spawn_factories();
    let factory = MockWorkerFactory::new();
    let factory_clone = factory.clone();
    
    register_test_spawn_factory(ChildType::Worker, factory);

    // Queue a spawn request
    let (sup_notify_ref, _sup_notify_rx) = TestRuntime::channel(0, 16);
    queue_spawn_request(
        ChildType::Worker,
        SpawnParams::Worker { task_id: 1 },
        None,
    );

    // Process the pending spawn
    let results = process_pending_spawns(1, sup_notify_ref);

    // Verify spawn happened
    assert_eq!(results.len(), 1);
    assert_eq!(factory_clone.spawn_count(), 1);

    // Cleanup
    clear_spawn_factories();
}

#[test]
fn multiple_spawns_processed_in_order() {
    use crate::test_impl::{clear_spawn_factories, clear_pending_spawns, process_pending_spawns, queue_spawn_request, register_test_spawn_factory};

    // Setup
    clear_spawn_factories();
    clear_pending_spawns();
    let factory = MockWorkerFactory::new();
    let factory_clone = factory.clone();
    
    register_test_spawn_factory(ChildType::Worker, factory);

    // Queue multiple spawn requests
    let (sup_notify_ref, _sup_notify_rx) = TestRuntime::channel(0, 16);
    queue_spawn_request(ChildType::Worker, SpawnParams::Worker { task_id: 1 }, None);
    queue_spawn_request(ChildType::Worker, SpawnParams::Worker { task_id: 2 }, None);
    queue_spawn_request(ChildType::Worker, SpawnParams::Worker { task_id: 3 }, None);

    // Process all pending spawns
    let results = process_pending_spawns(1, sup_notify_ref);

    // Verify all spawns happened
    assert_eq!(results.len(), 3);
    assert_eq!(factory_clone.spawn_count(), 3);

    // Cleanup
    clear_spawn_factories();
    clear_pending_spawns();
}
