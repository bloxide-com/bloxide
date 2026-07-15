// Copyright 2025 Bloxide, all rights reserved
//! Tests for dynamic spawning and peer introduction.

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec::Vec;

use bloxide_core::capability::{BloxRuntime, DynamicChannelCap};
use bloxide_core::lifecycle::{ChildLifecycleEvent, LifecycleCommand};
use bloxide_core::messaging::ActorRef;
use bloxide_core::test_utils::TestRuntime;

use crate::factory::{ErasedSpawnFactory, FactoryWrapper, SpawnCapability, SpawnFactoryFor};
use crate::output::{SpawnOutput, SpawnPolicy};
use crate::peer::{AddPeer, HasPeers, PeerCtrl, RemovePeer, introduce_peers};

// ── Domain message implementing SpawnCapability ────────────────────────────

#[derive(Clone, Debug)]
struct WorkerMsg;

impl SpawnCapability for WorkerMsg {
    type Params = WorkerParams;
}

#[derive(Clone, Debug)]
struct WorkerParams {
    task_id: u32,
}

struct WorkerFactory;

impl SpawnFactoryFor<WorkerMsg, TestRuntime> for WorkerFactory {
    fn spawn(
        &self,
        _supervisor_notify: ActorRef<ChildLifecycleEvent, TestRuntime>,
        params: WorkerParams,
    ) -> Option<SpawnOutput<TestRuntime>> {
        let child_id = TestRuntime::alloc_actor_id();
        let (lifecycle_ref, _rx) =
            TestRuntime::channel::<LifecycleCommand>(child_id, 16);
        let _ = params.task_id;
        Some(SpawnOutput {
            child_id,
            lifecycle_ref,
            policy: Some(SpawnPolicy::Restart { max: 3 }),
        })
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn spawn_factory_creates_child_with_output() {
    let factory = WorkerFactory;
    let (supervisor_notify, _rx) = TestRuntime::channel::<ChildLifecycleEvent>(0, 16);

    let output = factory.spawn(supervisor_notify, WorkerParams { task_id: 42 });
    assert!(output.is_some());

    let output = output.unwrap();
    assert_eq!(output.child_id, output.lifecycle_ref.id());
    assert!(matches!(
        output.policy,
        Some(SpawnPolicy::Restart { max: 3 })
    ));
}

#[test]
fn erased_spawn_factory_type_erasure_works() {
    #[derive(Clone, Debug)]
    struct OtherMsg;

    impl SpawnCapability for OtherMsg {
        type Params = OtherParams;
    }

    #[derive(Clone, Debug)]
    struct OtherParams {
        _0: u8,
    }

    struct OtherFactory;

    impl SpawnFactoryFor<OtherMsg, TestRuntime> for OtherFactory {
        fn spawn(
            &self,
            _supervisor_notify: ActorRef<ChildLifecycleEvent, TestRuntime>,
            _params: OtherParams,
        ) -> Option<SpawnOutput<TestRuntime>> {
            None
        }
    }

    let erased: Box<dyn ErasedSpawnFactory<TestRuntime>> =
        Box::new(FactoryWrapper::new(OtherFactory));
    let (supervisor_notify, _rx) = TestRuntime::channel::<ChildLifecycleEvent>(0, 16);

    // Passing WorkerParams to an OtherMsg factory should fail to downcast.
    let result = erased.spawn_erased(
        supervisor_notify,
        Box::new(WorkerParams { task_id: 1 }),
    );
    assert!(result.is_none());
}

// Simple peer-list container implementing HasPeers.
struct PeerList<M: Send + 'static, R: BloxRuntime> {
    peers: Vec<ActorRef<M, R>>,
}

impl<M: Send + 'static, R: BloxRuntime> HasPeers<M, R> for PeerList<M, R> {
    fn peers(&self) -> &[ActorRef<M, R>] {
        &self.peers
    }

    fn peers_mut(&mut self) -> &mut Vec<ActorRef<M, R>> {
        &mut self.peers
    }
}

#[test]
fn peer_ctrl_adds_and_removes_peers() {
    let id1 = TestRuntime::alloc_actor_id();
    let id2 = TestRuntime::alloc_actor_id();
    let (ref1, _rx1) = TestRuntime::channel::<WorkerMsg>(id1, 16);
    let (ref2, _rx2) = TestRuntime::channel::<WorkerMsg>(id2, 16);

    let mut list: PeerList<WorkerMsg, TestRuntime> = PeerList { peers: Vec::new() };
    list.peers_mut().push(ref1.clone());
    assert_eq!(list.peers().len(), 1);

    let add: PeerCtrl<WorkerMsg, TestRuntime> = PeerCtrl::AddPeer(AddPeer {
        peer_id: id2,
        peer_ref: ref2.clone(),
    });
    match add {
        PeerCtrl::AddPeer(add) => list.peers_mut().push(add.peer_ref),
        _ => panic!("expected AddPeer"),
    }
    assert_eq!(list.peers().len(), 2);

    let remove: PeerCtrl<WorkerMsg, TestRuntime> =
        PeerCtrl::RemovePeer(RemovePeer { peer_id: id1 });
    match remove {
        PeerCtrl::RemovePeer(remove) => list.peers_mut().retain(|r| r.id() != remove.peer_id),
        _ => panic!("expected RemovePeer"),
    }
    assert_eq!(list.peers().len(), 1);
    assert_eq!(list.peers()[0].id(), id2);
}

#[test]
fn introduce_peers_sends_add_peer_to_both() {
    let a_id = TestRuntime::alloc_actor_id();
    let b_id = TestRuntime::alloc_actor_id();

    let (a_ref, _a_rx) = TestRuntime::channel::<WorkerMsg>(a_id, 16);
    let (b_ref, _b_rx) = TestRuntime::channel::<WorkerMsg>(b_id, 16);
    let (a_ctrl, mut a_ctrl_rx) =
        TestRuntime::channel::<PeerCtrl<WorkerMsg, TestRuntime>>(a_id, 16);
    let (b_ctrl, mut b_ctrl_rx) =
        TestRuntime::channel::<PeerCtrl<WorkerMsg, TestRuntime>>(b_id, 16);

    let from = TestRuntime::alloc_actor_id();
    introduce_peers(
        from, a_id, a_ref, a_ctrl, b_id, b_ref, b_ctrl,
    );

    let a_msgs = a_ctrl_rx.drain_payloads();
    assert_eq!(a_msgs.len(), 1);
    match &a_msgs[0] {
        PeerCtrl::AddPeer(add) => assert_eq!(add.peer_id, b_id),
        _ => panic!("expected AddPeer on a_ctrl"),
    }

    let b_msgs = b_ctrl_rx.drain_payloads();
    assert_eq!(b_msgs.len(), 1);
    match &b_msgs[0] {
        PeerCtrl::AddPeer(add) => assert_eq!(add.peer_id, a_id),
        _ => panic!("expected AddPeer on b_ctrl"),
    }
}
