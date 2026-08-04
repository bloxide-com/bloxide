// Copyright 2025 Bloxide, all rights reserved
//! Tests for bloxide-peers helpers.

use crate::*;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use bloxide_core::capability::DynamicChannelCap;
use bloxide_test_runtime::TestRuntime;

// A simple message type for testing.
#[derive(Clone, Debug, PartialEq)]
struct TestMsg {
    payload: u32,
}

// ── introduce_peers ────────────────────────────────────────────────────────

#[test]
fn introduce_peers_sends_add_peer_to_both() {
    let a_id = 1usize;
    let b_id = 2usize;

    let (a_ref, _a_rx) = TestRuntime::channel::<TestMsg>(a_id, 4);
    let (b_ref, _b_rx) = TestRuntime::channel::<TestMsg>(b_id, 4);
    let (a_ctrl, mut a_ctrl_rx) = TestRuntime::channel::<PeerCtrl<TestMsg, TestRuntime>>(a_id + 100, 4);
    let (b_ctrl, mut b_ctrl_rx) = TestRuntime::channel::<PeerCtrl<TestMsg, TestRuntime>>(b_id + 100, 4);

    let result = introduce_peers(0, a_id, a_ref, a_ctrl, b_id, b_ref, b_ctrl);
    assert_eq!(result, ActionResult::Ok, "introduce_peers should succeed: {:?}", result);

    let a_ctrl_msgs = a_ctrl_rx.drain_payloads();
    assert_eq!(a_ctrl_msgs.len(), 1);
    assert!(
        matches!(
            &a_ctrl_msgs[0],
            PeerCtrl::AddPeer(AddPeer { peer_id, .. }) if *peer_id == b_id
        ),
        "a_ctrl should receive AddPeer for b, got {:?}",
        a_ctrl_msgs
    );

    let b_ctrl_msgs = b_ctrl_rx.drain_payloads();
    assert_eq!(b_ctrl_msgs.len(), 1);
    assert!(
        matches!(
            &b_ctrl_msgs[0],
            PeerCtrl::AddPeer(AddPeer { peer_id, .. }) if *peer_id == a_id
        ),
        "b_ctrl should receive AddPeer for a, got {:?}",
        b_ctrl_msgs
    );
}

#[test]
fn introduce_peers_returns_err_if_first_send_fails() {
    let a_id = 1usize;
    let b_id = 2usize;

    let (a_ref, _a_rx) = TestRuntime::channel::<TestMsg>(a_id, 4);
    let (b_ref, _b_rx) = TestRuntime::channel::<TestMsg>(b_id, 4);
    // Fill a_ctrl so the first send fails.
    let (a_ctrl, _a_ctrl_rx) = TestRuntime::channel::<PeerCtrl<TestMsg, TestRuntime>>(a_id + 100, 0);
    let (b_ctrl, mut b_ctrl_rx) = TestRuntime::channel::<PeerCtrl<TestMsg, TestRuntime>>(b_id + 100, 4);

    let result = introduce_peers(0, a_id, a_ref, a_ctrl, b_id, b_ref, b_ctrl);
    assert_eq!(result, ActionResult::Err, "introduce_peers should fail when first send fails");
    // The second send still happens (best-effort).
    let b_ctrl_msgs = b_ctrl_rx.drain_payloads();
    assert_eq!(b_ctrl_msgs.len(), 1);
}

// ── apply_peer_control ───────────────────────────────────────────────────────

#[test]
fn apply_peer_control_adds_peer() {
    let mut peers: Vec<bloxide_core::messaging::ActorRef<TestMsg, TestRuntime>> = Vec::new();
    let peer_id = 7usize;
    let (peer_ref, _rx) = TestRuntime::channel::<TestMsg>(peer_id, 4);

    let ctrl = PeerCtrl::AddPeer(AddPeer { peer_id, peer_ref });
    let result = apply_peer_control(&mut peers, &ctrl);
    assert_eq!(result, ActionResult::Ok);
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].id(), peer_id);
}

#[test]
fn apply_peer_control_add_is_idempotent() {
    let mut peers: Vec<bloxide_core::messaging::ActorRef<TestMsg, TestRuntime>> = Vec::new();
    let peer_id = 7usize;
    let (peer_ref, _rx) = TestRuntime::channel::<TestMsg>(peer_id, 4);

    let ctrl = PeerCtrl::AddPeer(AddPeer {
        peer_id,
        peer_ref: peer_ref.clone(),
    });
    apply_peer_control(&mut peers, &ctrl);
    apply_peer_control(&mut peers, &ctrl);
    assert_eq!(peers.len(), 1, "duplicate AddPeer should not add again");
}

#[test]
fn apply_peer_control_removes_peer() {
    let mut peers: Vec<bloxide_core::messaging::ActorRef<TestMsg, TestRuntime>> = Vec::new();
    let id1 = 7usize;
    let id2 = 8usize;
    let (r1, _rx1) = TestRuntime::channel::<TestMsg>(id1, 4);
    let (r2, _rx2) = TestRuntime::channel::<TestMsg>(id2, 4);

    apply_peer_control(&mut peers, &PeerCtrl::AddPeer(AddPeer { peer_id: id1, peer_ref: r1 }));
    apply_peer_control(&mut peers, &PeerCtrl::AddPeer(AddPeer { peer_id: id2, peer_ref: r2 }));
    assert_eq!(peers.len(), 2);

    let result = apply_peer_control(&mut peers, &PeerCtrl::RemovePeer(RemovePeer { peer_id: id1 }));
    assert_eq!(result, ActionResult::Ok);
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].id(), id2);
}

// ── broadcast_to_peers ───────────────────────────────────────────────────────

#[test]
fn broadcast_to_peers_delivers_to_all() {
    let id1 = 1usize;
    let id2 = 2usize;
    let (r1, mut rx1) = TestRuntime::channel::<TestMsg>(id1, 4);
    let (r2, mut rx2) = TestRuntime::channel::<TestMsg>(id2, 4);
    let peers = vec![r1, r2];

    let msg = TestMsg { payload: 42 };
    let result = broadcast_to_peers(0, &peers, msg.clone());
    assert_eq!(result, ActionResult::Ok);

    let msgs1 = rx1.drain_payloads();
    let msgs2 = rx2.drain_payloads();
    assert_eq!(msgs1.len(), 1);
    assert_eq!(msgs2.len(), 1);
    assert_eq!(msgs1[0], msg);
    assert_eq!(msgs2[0], msg);
}

#[test]
fn broadcast_to_peers_returns_err_on_any_failure() {
    let id1 = 1usize;
    let id2 = 2usize;
    let (r1, mut rx1) = TestRuntime::channel::<TestMsg>(id1, 4);
    // Capacity 0 so send fails.
    let (r2, _rx2) = TestRuntime::channel::<TestMsg>(id2, 0);
    let peers = vec![r1, r2];

    let msg = TestMsg { payload: 42 };
    let result = broadcast_to_peers(0, &peers, msg.clone());
    assert_eq!(result, ActionResult::Err, "broadcast should fail when any send fails");

    // Best-effort: the first send still landed.
    let msgs1 = rx1.drain_payloads();
    assert_eq!(msgs1.len(), 1);
}

#[test]
fn broadcast_to_peers_empty_is_ok() {
    let peers: Vec<bloxide_core::messaging::ActorRef<TestMsg, TestRuntime>> = Vec::new();
    let result = broadcast_to_peers(0, &peers, TestMsg { payload: 0 });
    assert_eq!(result, ActionResult::Ok);
}

// ── Clone + Debug ────────────────────────────────────────────────────────────

#[test]
fn peer_ctrl_clone_roundtrips() {
    let peer_id = 5usize;
    let (peer_ref, _rx) = TestRuntime::channel::<TestMsg>(peer_id, 4);
    let original = PeerCtrl::AddPeer(AddPeer { peer_id, peer_ref });
    let cloned = original.clone();
    assert!(
        matches!(cloned, PeerCtrl::AddPeer(AddPeer { peer_id: id, .. }) if id == peer_id)
    );
}

#[test]
fn remove_peer_debug_format() {
    let rp = RemovePeer { peer_id: 99usize };
    let s = format!("{:?}", rp);
    assert!(s.contains("99"));
}

#[test]
fn add_peer_debug_format() {
    let peer_id = 99usize;
    let (peer_ref, _rx) = TestRuntime::channel::<TestMsg>(peer_id, 4);
    let ap = AddPeer { peer_id, peer_ref };
    let s = format!("{:?}", ap);
    assert!(s.contains("99"));
    assert!(s.contains("peer_ref_id"));
}

#[test]
fn peer_ctrl_debug_format() {
    let peer_id = 99usize;
    let (peer_ref, _rx) = TestRuntime::channel::<TestMsg>(peer_id, 4);
    let pc = PeerCtrl::<TestMsg, TestRuntime>::AddPeer(AddPeer { peer_id, peer_ref });
    let s = format!("{:?}", pc);
    assert!(s.contains("AddPeer"));
}
