// Copyright 2025 Bloxide, all rights reserved
//! Tests for `ChildGroup` — confirm-before-record reliability semantics.

use crate::*;
use bloxide_core::capability::{BloxRuntime, DynamicChannelCap};
use bloxide_core::messaging::Envelope;
use bloxide_spawn::SpawnCap;
use bloxide_test_runtime::{TestReceiver, TestRuntime};

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

const FROM: usize = 100;

fn setup_one_child(
    policy: ChildPolicy,
) -> (
    ChildGroup<TestRuntime>,
    TestReceiver<LifecycleCommand>,
    ActorRef<ChildLifecycleEvent, TestRuntime>,
    TestReceiver<ChildLifecycleEvent>,
) {
    let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let id = 1usize;
    let (lifecycle_ref, rx) = TestRuntime::channel::<LifecycleCommand>(id, 16);
    let (notify_ref, notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group.try_add(id, lifecycle_ref, policy).unwrap();
    (group, rx, notify_ref, notify_rx)
}

/// A group with one dynamic child (abort mailbox + recorded kill handle).
/// Returns the spawn id used as kill handle.
fn setup_dynamic_child(
    policy: ChildPolicy,
) -> (
    ChildGroup<TestRuntime>,
    TestReceiver<LifecycleCommand>,
    TestReceiver<AbortCommand>,
    ActorRef<ChildLifecycleEvent, TestRuntime>,
    TestReceiver<ChildLifecycleEvent>,
    usize,
) {
    let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let id = 1usize;
    let (lifecycle_ref, lc_rx) = TestRuntime::channel::<LifecycleCommand>(id, 16);
    let (abort_ref, abort_rx) = TestRuntime::channel::<AbortCommand>(id + 100, 16);
    let (notify_ref, notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    let task = TestRuntime::spawn(async {});
    let kill_handle = TestRuntime::kill_handle(task);
    group
        .try_add_dynamic(id, lifecycle_ref, abort_ref, kill_handle, policy)
        .unwrap();
    (group, lc_rx, abort_rx, notify_ref, notify_rx, kill_handle)
}

// ── Registration validation ─────────────────────────────────────────────

#[test]
fn try_add_rejects_abort_and_kill_policies() {
    let mut group = ChildGroup::<TestRuntime>::new(GroupShutdown::WhenAnyDone);
    let (r1, _rx1) = TestRuntime::channel::<LifecycleCommand>(1, 4);
    let (r2, _rx2) = TestRuntime::channel::<LifecycleCommand>(2, 4);
    assert_eq!(
        group.try_add(1, r1, ChildPolicy::Abort),
        Err(RegistrationError::PolicyRequiresHandles)
    );
    assert_eq!(
        group.try_add(2, r2, ChildPolicy::Kill),
        Err(RegistrationError::PolicyRequiresHandles)
    );
}

#[test]
fn try_add_rejects_duplicate_registration() {
    let (mut group, _rx, _notify_ref, _notify_rx) = setup_one_child(ChildPolicy::Reset { max: 3 });
    let (dup_ref, _dup_rx) = TestRuntime::channel::<LifecycleCommand>(1, 4);
    assert_eq!(
        group.try_add(1, dup_ref, ChildPolicy::Stop),
        Err(RegistrationError::Duplicate),
        "a second entry for the same id corrupts bookkeeping — must be refused"
    );
    assert_eq!(group.children.len(), 1);
}

#[test]
fn try_add_dynamic_rejects_duplicate_registration() {
    let (mut group, _lc_rx, _abort_rx, _notify_ref, _notify_rx, _kh) =
        setup_dynamic_child(ChildPolicy::Abort);
    let (dup_ref, _dup_rx) = TestRuntime::channel::<LifecycleCommand>(1, 4);
    let (dup_abort, _dup_abort_rx) = TestRuntime::channel::<AbortCommand>(101, 4);
    assert_eq!(
        group.try_add_dynamic(1, dup_ref, dup_abort, 42, ChildPolicy::Abort),
        Err(RegistrationError::Duplicate)
    );
    assert_eq!(group.children.len(), 1);
}

/// A runtime whose kill capability is a no-op (`NoKill`), reusing the
/// TestRuntime channel machinery. Only `Kill` differs.
#[derive(Clone)]
struct NoKillRuntime;

impl BloxRuntime for NoKillRuntime {
    type SendError = bloxide_test_runtime::TestSendError;
    type TrySendError = bloxide_test_runtime::TestTrySendError;
    type Sender<M: Send + 'static> = bloxide_test_runtime::TestSender<M>;
    type Receiver<M: Send + 'static> = bloxide_test_runtime::TestReceiver<M>;
    type Stream<M: Send + 'static> = bloxide_test_runtime::TestReceiver<M>;
    type Kill = bloxide_core::capability::NoKill;

    fn to_stream<M: Send + 'static>(rx: Self::Receiver<M>) -> Self::Stream<M> {
        rx
    }

    async fn send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::SendError> {
        TestRuntime::send_via(sender, envelope).await
    }

    fn try_send_via<M: Send + 'static>(
        sender: &Self::Sender<M>,
        envelope: Envelope<M>,
    ) -> Result<(), Self::TrySendError> {
        TestRuntime::try_send_via(sender, envelope)
    }

    fn try_send_error_is_closed(err: &Self::TrySendError) -> bool {
        TestRuntime::try_send_error_is_closed(err)
    }
}

impl DynamicChannelCap for NoKillRuntime {
    fn alloc_actor_id() -> bloxide_core::messaging::ActorId {
        TestRuntime::alloc_actor_id()
    }

    fn channel<M: Send + 'static>(
        id: bloxide_core::messaging::ActorId,
        capacity: usize,
    ) -> (
        bloxide_core::messaging::ActorRef<M, Self>,
        Self::Receiver<M>,
    ) {
        // Reuse the TestRuntime channel machinery; re-wrap the sender under
        // the NoKillRuntime type so `ActorRef<M, NoKillRuntime>` checks.
        let (aref, rx) = TestRuntime::channel::<M>(id, capacity);
        let sender = aref.sender();
        (bloxide_core::messaging::ActorRef::new(id, sender), rx)
    }
}

#[test]
fn try_add_dynamic_rejects_kill_policy_on_nokill_runtime() {
    let mut group = ChildGroup::<NoKillRuntime>::new(GroupShutdown::WhenAnyDone);
    let (lifecycle_ref, _rx) = NoKillRuntime::channel::<LifecycleCommand>(1, 4);
    let (abort_ref, _abort_rx) = NoKillRuntime::channel::<AbortCommand>(101, 4);
    assert_eq!(
        group.try_add_dynamic(1, lifecycle_ref, abort_ref, (), ChildPolicy::Kill),
        Err(RegistrationError::KillUnavailable),
        "Kill under NoKill marks a live child dead — must be refused at registration"
    );
    assert!(group.children.is_empty());
}

// ── Reset policy: delivery, cap, and confirm-before-record ──────────────

#[test]
fn reset_policy_sends_reset_and_continues() {
    let (mut group, mut rx, notify_ref, _notify_rx) =
        setup_one_child(ChildPolicy::Reset { max: 3 });

    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::Continue);
    let cmds = rx.drain_payloads();
    assert_eq!(cmds.len(), 1, "exactly one Reset command expected");
    assert!(matches!(cmds[0], LifecycleCommand::Reset));
    assert_eq!(group.children[0].phase, ChildPhase::ResetPending);
    assert_eq!(group.children[0].restarts, 1);
}

#[test]
fn duplicate_done_while_reset_pending_is_coalesced() {
    let (mut group, mut rx, notify_ref, _notify_rx) =
        setup_one_child(ChildPolicy::Reset { max: 3 });

    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::Continue);
    assert_eq!(rx.drain_payloads().len(), 1); // Reset sent

    // Second failure while ResetPending → coalesced (no second Reset)
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::Continue);
    assert_eq!(rx.drain_payloads().len(), 0); // nothing sent
}

#[test]
fn reset_cap_gives_up_after_max_consecutive_restarts() {
    let (mut group, mut rx, notify_ref, _notify_rx) =
        setup_one_child(ChildPolicy::Reset { max: 2 });

    // Two restarts allowed
    group.handle_done_or_failed(1, FROM, &notify_ref);
    group.handle_started(1, FROM); // revive, but no Alive yet — still consecutive
    group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(rx.drain_payloads().len(), 2);
    assert_eq!(group.children[0].restarts, 2);

    // Third consecutive failure — cap exhausted: give up (terminal Stopped),
    // no Reset sent, group shutdown triggered (WhenAnyDone).
    group.handle_started(1, FROM);
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::BeginShutdown);
    assert_eq!(rx.drain_payloads().len(), 0, "no Reset after cap exhausted");
    assert_eq!(group.children[0].phase, ChildPhase::Stopped);
}

#[test]
fn alive_after_started_resets_consecutive_restart_counter() {
    let (mut group, mut rx, notify_ref, _notify_rx) =
        setup_one_child(ChildPolicy::Reset { max: 1 });

    group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(rx.drain_payloads().len(), 1);
    assert_eq!(group.children[0].restarts, 1);

    // Child revives and proves sustained uptime (Alive) → counter resets.
    group.handle_started(1, FROM);
    group.handle_alive(1);
    assert_eq!(group.children[0].restarts, 0);

    // A later failure restarts the count from zero — Reset allowed again.
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::Continue);
    assert_eq!(rx.drain_payloads().len(), 1);
    assert_eq!(group.children[0].restarts, 1);
}

#[test]
fn started_alone_does_not_reset_restart_counter() {
    // Otherwise a crash loop (fail → Reset → Started → fail ...) would keep
    // resetting its own cap. The counter requires an Alive (uptime proof).
    let (mut group, mut rx, notify_ref, _notify_rx) =
        setup_one_child(ChildPolicy::Reset { max: 1 });

    group.handle_done_or_failed(1, FROM, &notify_ref);
    group.handle_started(1, FROM);
    assert_eq!(group.children[0].restarts, 1);

    // Second consecutive failure with max = 1 → give up.
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::BeginShutdown);
    assert_eq!(rx.drain_payloads().len(), 1, "only one Reset total");
    assert_eq!(group.children[0].phase, ChildPhase::Stopped);
}

#[test]
fn failed_reset_send_is_queued_not_reset_pending() {
    // Confirm-before-record: a Reset that could not be delivered must NOT
    // move the child to ResetPending (the old code wedged the child there
    // forever — no Started could ever arrive).
    let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let (lifecycle_ref, _rx) = TestRuntime::channel::<LifecycleCommand>(1, 0); // always full
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group
        .try_add(1, lifecycle_ref, ChildPolicy::Reset { max: 3 })
        .unwrap();
    group.handle_started(1, FROM);

    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::Continue);
    assert_eq!(
        group.children[0].phase,
        ChildPhase::Running,
        "undelivered Reset must not change the phase"
    );
    assert_eq!(group.children[0].pending_cmd, Some(PendingCmd::Reset));
    assert_eq!(
        group.children[0].restarts, 0,
        "restart counted only on delivery"
    );
}

#[test]
fn flush_pending_delivers_queued_reset_and_counts_restart() {
    let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let (lifecycle_ref, mut rx) = TestRuntime::channel::<LifecycleCommand>(1, 1);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group
        .try_add(1, lifecycle_ref.clone(), ChildPolicy::Reset { max: 3 })
        .unwrap();
    group.handle_started(1, FROM);

    // Fill the channel so the Reset cannot be delivered immediately.
    lifecycle_ref
        .try_send(FROM, LifecycleCommand::Ping)
        .unwrap();
    group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(group.children[0].pending_cmd, Some(PendingCmd::Reset));
    assert_eq!(group.children[0].phase, ChildPhase::Running);

    // Flush while still full — stays queued, still Running.
    let action = group.flush_pending(FROM);
    assert_eq!(action, ChildAction::Continue);
    assert_eq!(group.children[0].pending_cmd, Some(PendingCmd::Reset));

    // Drain; flush now delivers — ResetPending and restart counted.
    rx.drain_payloads();
    let action = group.flush_pending(FROM);
    assert_eq!(action, ChildAction::Continue);
    assert_eq!(group.children[0].pending_cmd, None);
    assert_eq!(group.children[0].phase, ChildPhase::ResetPending);
    assert_eq!(group.children[0].restarts, 1);
    assert!(matches!(rx.drain_payloads()[0], LifecycleCommand::Reset));
}

#[test]
fn flush_pending_recovers_start_for_never_started_child() {
    // Registration-time Start lost to a full channel: queued, retried on the
    // next event pass, delivered once the channel drains.
    let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let (lifecycle_ref, mut rx) = TestRuntime::channel::<LifecycleCommand>(1, 1);
    let (_notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group
        .try_add(1, lifecycle_ref.clone(), ChildPolicy::Stop)
        .unwrap();

    lifecycle_ref
        .try_send(FROM, LifecycleCommand::Ping)
        .unwrap(); // fill
    group.start_child(1, FROM);
    assert_eq!(group.children[0].pending_cmd, Some(PendingCmd::Start));

    rx.drain_payloads();
    group.flush_pending(FROM);
    assert_eq!(group.children[0].pending_cmd, None);
    assert!(matches!(rx.drain_payloads()[0], LifecycleCommand::Start));
}

#[test]
fn failed_reset_send_on_closed_channel_marks_gone() {
    let mut group = ChildGroup::new(GroupShutdown::WhenAllDone);
    let (lifecycle_ref, rx) = TestRuntime::channel::<LifecycleCommand>(1, 4);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group
        .try_add(1, lifecycle_ref, ChildPolicy::Reset { max: 3 })
        .unwrap();
    group.handle_started(1, FROM);

    drop(rx); // child task dead — channel closed
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(group.children[0].phase, ChildPhase::Gone);
    assert_eq!(
        action,
        ChildAction::BeginShutdown,
        "Gone is terminal — WhenAllDone completes when the last child is gone"
    );
}

// ── Stop / Abort / Kill policies ────────────────────────────────────────

#[test]
fn stop_policy_sets_stopped_and_triggers_shutdown() {
    let (mut group, mut rx, notify_ref, _notify_rx) = setup_one_child(ChildPolicy::Stop);

    group.handle_started(1, FROM);
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::BeginShutdown);
    assert_eq!(group.children[0].phase, ChildPhase::Stopped);
    assert_eq!(rx.drain_payloads().len(), 0);
}

#[test]
fn stop_policy_with_when_all_done_waits_for_others() {
    let mut group = ChildGroup::new(GroupShutdown::WhenAllDone);
    let (lifecycle_ref1, _rx1) = TestRuntime::channel::<LifecycleCommand>(1, 16);
    let (lifecycle_ref2, _rx2) = TestRuntime::channel::<LifecycleCommand>(2, 16);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group.try_add(1, lifecycle_ref1, ChildPolicy::Stop).unwrap();
    group.try_add(2, lifecycle_ref2, ChildPolicy::Stop).unwrap();

    group.handle_started(1, FROM);
    group.handle_started(2, FROM);

    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::Continue);

    let action = group.handle_done_or_failed(2, FROM, &notify_ref);
    assert_eq!(action, ChildAction::BeginShutdown);
}

#[test]
fn abort_policy_sends_abort_and_waits_in_aborting() {
    let (mut group, _lc_rx, mut abort_rx, notify_ref, _notify_rx, _kh) =
        setup_dynamic_child(ChildPolicy::Abort);
    group.handle_started(1, FROM);

    // Confirm-before-record: delivered Abort → Aborting (NOT terminal) and
    // no shutdown yet — the Aborted report finalizes.
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::Continue);
    assert_eq!(group.children[0].phase, ChildPhase::Aborting);

    let cmds = abort_rx.drain_payloads();
    assert_eq!(cmds.len(), 1, "exactly one AbortCommand expected");
    assert!(matches!(cmds[0], AbortCommand::Abort { child_id: 1 }));

    // Policy must not re-fire while the abort is in flight.
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::Continue);
    assert_eq!(abort_rx.drain_payloads().len(), 0);

    // The Aborted report finalizes → terminal → shutdown (WhenAnyDone).
    let action = group.record_aborted(1, FROM);
    assert_eq!(action, ChildAction::BeginShutdown);
    assert_eq!(group.children[0].phase, ChildPhase::Aborted);
}

#[test]
fn failed_abort_send_is_queued_then_flushed() {
    let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let (lifecycle_ref, _lc_rx) = TestRuntime::channel::<LifecycleCommand>(1, 16);
    let (abort_ref, mut abort_rx) = TestRuntime::channel::<AbortCommand>(101, 1);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group
        .try_add_dynamic(1, lifecycle_ref, abort_ref.clone(), 7, ChildPolicy::Abort)
        .unwrap();
    group.handle_started(1, FROM);

    // Fill the abort channel → Abort queued, child stays Running (not Aborted —
    // the old code marked Aborted on a failed send, leaking a live child).
    abort_ref
        .try_send(FROM, AbortCommand::Abort { child_id: 999 })
        .unwrap();
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::Continue);
    assert_eq!(group.children[0].phase, ChildPhase::Running);
    assert_eq!(group.children[0].pending_cmd, Some(PendingCmd::Abort));

    // Drain; flush delivers → Aborting.
    abort_rx.drain_payloads();
    group.flush_pending(FROM);
    assert_eq!(group.children[0].phase, ChildPhase::Aborting);
    assert_eq!(group.children[0].pending_cmd, None);
    assert!(matches!(
        abort_rx.drain_payloads()[..],
        [AbortCommand::Abort { child_id: 1 }]
    ));
}

#[test]
fn abort_send_on_closed_channel_marks_gone() {
    let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let (lifecycle_ref, _lc_rx) = TestRuntime::channel::<LifecycleCommand>(1, 16);
    let (abort_ref, abort_rx) = TestRuntime::channel::<AbortCommand>(101, 16);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group
        .try_add_dynamic(1, lifecycle_ref, abort_ref, 7, ChildPolicy::Abort)
        .unwrap();
    group.handle_started(1, FROM);

    drop(abort_rx); // abort mailbox dead → child task gone
    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::BeginShutdown);
    assert_eq!(group.children[0].phase, ChildPhase::Gone);
}

#[test]
fn kill_policy_kills_task_and_emits_killed_event() {
    let (mut group, _lc_rx, _abort_rx, notify_ref, mut notify_rx, kill_handle) =
        setup_dynamic_child(ChildPolicy::Kill);
    group.handle_started(1, FROM);
    bloxide_test_runtime::drain_killed();

    let action = group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(action, ChildAction::BeginShutdown);
    assert_eq!(group.children[0].phase, ChildPhase::Killed);

    // The ripcord actually fired (observable kill).
    assert_eq!(
        bloxide_test_runtime::drain_killed(),
        vec![kill_handle],
        "the stored kill handle must be consumed by the ripcord"
    );

    // And the Killed event was synthesized on the notify channel.
    let events = notify_rx.drain_payloads();
    assert!(
        matches!(events[..], [ChildLifecycleEvent::Killed { child_id: 1 }]),
        "expected Killed event, got {:?}",
        events
    );
}

// ── Health checks ───────────────────────────────────────────────────────

#[test]
fn health_tick_pings_child_and_counts_misses_before_rogue() {
    let (mut group, mut rx, notify_ref, _notify_rx) =
        setup_one_child(ChildPolicy::Reset { max: 3 });
    group.handle_started(1, FROM);

    // Tick 1: first Ping delivered, no verdict yet.
    group.health_check_tick(FROM, &notify_ref);
    assert!(matches!(rx.drain_payloads()[..], [LifecycleCommand::Ping]));
    assert_eq!(group.children[0].misses, 0); // no outstanding ping before this one

    // Tick 2: one unanswered Ping → miss 1 (below MAX_MISSES) → no Reset.
    group.health_check_tick(FROM, &notify_ref);
    assert_eq!(rx.drain_payloads().len(), 1, "re-pinged, not reset");

    // Tick 3: second consecutive miss → rogue → policy (Reset).
    group.health_check_tick(FROM, &notify_ref);
    let cmds = rx.drain_payloads();
    assert!(
        cmds.iter().any(|c| matches!(c, LifecycleCommand::Reset)),
        "two consecutive misses must declare the child rogue, got {:?}",
        cmds
    );
}

#[test]
fn full_ping_channel_is_not_counted_as_miss() {
    // A Ping that was never delivered must not count against the child —
    // otherwise a transient full channel produces a false rogue verdict and
    // can reset (or kill) a healthy child.
    let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
    let (lifecycle_ref, mut rx) = TestRuntime::channel::<LifecycleCommand>(1, 1);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group
        .try_add(1, lifecycle_ref.clone(), ChildPolicy::Reset { max: 3 })
        .unwrap();
    group.handle_started(1, FROM);

    // Tick 1: Ping delivered (fills capacity 1), outstanding.
    group.health_check_tick(FROM, &notify_ref);
    // Tick 2: outstanding + send fails Full → one miss (from the outstanding
    // ping), but no NEW miss from the undelivered one.
    group.health_check_tick(FROM, &notify_ref);
    assert_eq!(group.children[0].misses, 1);
    assert_eq!(group.children[0].phase, ChildPhase::Running);

    // Tick 3: still full — the original outstanding Ping convicts (miss 2 →
    // rogue), but the Reset remedy also cannot be delivered: it is queued in
    // pending_cmd, and the phase stays Running (confirm-before-record).
    group.health_check_tick(FROM, &notify_ref);
    assert_eq!(group.children[0].phase, ChildPhase::Running);
    assert_eq!(group.children[0].pending_cmd, Some(PendingCmd::Reset));

    // Drain; the flush delivers the queued Reset.
    rx.drain_payloads();
    group.flush_pending(FROM);
    assert_eq!(group.children[0].phase, ChildPhase::ResetPending);
    assert!(matches!(rx.drain_payloads()[..], [LifecycleCommand::Reset]));
}

#[test]
fn closed_ping_channel_marks_child_gone() {
    let mut group = ChildGroup::new(GroupShutdown::WhenAllDone);
    let (lifecycle_ref, rx) = TestRuntime::channel::<LifecycleCommand>(1, 4);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);
    group
        .try_add(1, lifecycle_ref, ChildPolicy::Reset { max: 3 })
        .unwrap();
    group.handle_started(1, FROM);

    drop(rx);
    let action = group.health_check_tick(FROM, &notify_ref);
    assert_eq!(group.children[0].phase, ChildPhase::Gone);
    assert_eq!(action, ChildAction::BeginShutdown);
}

#[test]
fn reset_pending_child_is_pinged_and_alive_heals_lost_started() {
    let (mut group, mut rx, notify_ref, _notify_rx) =
        setup_one_child(ChildPolicy::Reset { max: 3 });

    // Trigger Reset → ResetPending (awaiting Started).
    group.handle_done_or_failed(1, FROM, &notify_ref);
    assert_eq!(rx.drain_payloads().len(), 1);

    // Health check pings ResetPending children (they are transitioning, and a
    // lost Started report must not wedge them — the old code excluded them,
    // so a dropped Started left the child in ResetPending forever).
    group.health_check_tick(FROM, &notify_ref);
    assert!(matches!(rx.drain_payloads()[..], [LifecycleCommand::Ping]));

    // Alive proves the child is operational → heals ResetPending to Running
    // even if the Started report itself was lost.
    group.handle_alive(1);
    assert_eq!(group.children[0].phase, ChildPhase::Running);
}

#[test]
fn alive_heals_init_phase_after_lost_started_report() {
    // Registration-time Start was delivered but the Started report was lost:
    // the next Ping/Alive round brings the child to Running.
    let (mut group, _rx, notify_ref, _notify_rx) = setup_one_child(ChildPolicy::Stop);
    assert_eq!(group.children[0].phase, ChildPhase::Init);

    group.health_check_tick(FROM, &notify_ref);
    group.handle_alive(1);
    assert_eq!(group.children[0].phase, ChildPhase::Running);
}

// ── Record functions (ShuttingDown / external events) ───────────────────

#[test]
fn record_stopped_never_overwrites_terminal_phase() {
    let (mut group, _rx, _notify_ref, _notify_rx) = setup_one_child(ChildPolicy::Stop);
    group.handle_started(1, FROM);
    group.record_killed(1, FROM);
    assert_eq!(group.children[0].phase, ChildPhase::Killed);

    // A late Stopped must not resurrect a task-gone phase.
    group.record_stopped(1, FROM);
    assert_eq!(group.children[0].phase, ChildPhase::Killed);
}

#[test]
fn record_aborted_and_killed_guard_terminal_and_evaluate_shutdown() {
    let mut group = ChildGroup::new(GroupShutdown::WhenAllDone);
    let (r1, _rx1) = TestRuntime::channel::<LifecycleCommand>(1, 4);
    let (r2, _rx2) = TestRuntime::channel::<LifecycleCommand>(2, 4);
    group.try_add(1, r1, ChildPolicy::Stop).unwrap();
    group.try_add(2, r2, ChildPolicy::Stop).unwrap();
    group.handle_started(1, FROM);
    group.handle_started(2, FROM);

    // Externally-originated Aborted participates in group shutdown.
    let action = group.record_aborted(1, FROM);
    assert_eq!(action, ChildAction::Continue); // child 2 still running

    // record_stopped on the Aborted child is a no-op (terminal guard).
    group.record_stopped(1, FROM);
    assert_eq!(group.children[0].phase, ChildPhase::Aborted);

    // Last child killed externally → shutdown.
    let action = group.record_killed(2, FROM);
    assert_eq!(action, ChildAction::BeginShutdown);
    assert!(group.all_stopped());

    // A duplicate Killed on a terminal child: no state change, no re-eval.
    let action = group.record_killed(2, FROM);
    assert_eq!(action, ChildAction::Continue);
}

#[test]
fn record_failed_marks_stopped_for_shutdown_accounting() {
    let (mut group, _rx, _notify_ref, _notify_rx) = setup_one_child(ChildPolicy::Stop);
    group.handle_started(1, FROM);

    group.record_failed(1, FROM);
    // Failed-parked children have a live task → Stopped (terminal, task alive).
    assert_eq!(group.children[0].phase, ChildPhase::Stopped);
    assert!(group.all_stopped());
}

#[test]
fn unknown_child_events_are_ignored_without_shutdown_eval() {
    let (mut group, _rx, notify_ref, _notify_rx) = setup_one_child(ChildPolicy::Stop);
    group.handle_started(1, FROM);

    // Unknown-child events must never trigger shutdown — WhenAnyDone would
    // otherwise fire on a forged or miswired report.
    assert_eq!(
        group.handle_done_or_failed(999, FROM, &notify_ref),
        ChildAction::Continue
    );
    assert_eq!(group.record_aborted(999, FROM), ChildAction::Continue);
    assert_eq!(group.record_killed(999, FROM), ChildAction::Continue);
    assert_eq!(group.deregister(999, FROM), ChildAction::Continue);
    assert_eq!(group.children.len(), 1);
    assert!(!group.all_stopped());
}

#[test]
fn deregister_known_child_evaluates_shutdown() {
    let (mut group, _rx, _notify_ref, _notify_rx) = setup_one_child(ChildPolicy::Stop);
    group.handle_started(1, FROM);

    // Known Done → removed → WhenAnyDone shutdown. Unknown Done (above) does not.
    let action = group.deregister(1, FROM);
    assert_eq!(action, ChildAction::BeginShutdown);
    assert!(group.children.is_empty());
}

#[test]
fn clear_counters_preserves_terminal_phases_and_clears_pending() {
    let mut group = ChildGroup::new(GroupShutdown::WhenAllDone);
    let (notify_ref, _notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(FROM, 16);

    let mut receivers = Vec::new();
    for id in 1..=4usize {
        let (lifecycle_ref, rx) = TestRuntime::channel::<LifecycleCommand>(id, 16);
        group
            .try_add(id, lifecycle_ref, ChildPolicy::Reset { max: 3 })
            .unwrap();
        receivers.push(rx);
    }

    group.handle_started(1, FROM);
    group.handle_started(2, FROM);
    group.record_stopped(2, FROM);
    group.record_aborted(3, FROM);
    group.record_killed(4, FROM);

    group.clear_counters();

    assert_eq!(group.children[0].phase, ChildPhase::Init);
    assert_eq!(group.children[1].phase, ChildPhase::Stopped);
    assert_eq!(group.children[2].phase, ChildPhase::Aborted);
    assert_eq!(group.children[3].phase, ChildPhase::Killed);

    // Health check pings only the reset child.
    group.health_check_tick(FROM, &notify_ref);
    let pings: Vec<usize> = receivers
        .iter_mut()
        .map(|rx| rx.drain_payloads().len())
        .collect();
    assert_eq!(pings, [1, 0, 0, 0]);
}

// ── start_all / stop_all confirm-before-record ──────────────────────────

#[test]
fn stop_all_skips_task_gone_and_queues_full_sends() {
    let mut group = ChildGroup::new(GroupShutdown::WhenAllDone);
    let (r1, mut rx1) = TestRuntime::channel::<LifecycleCommand>(1, 1);
    let (r2, rx2) = TestRuntime::channel::<LifecycleCommand>(2, 4);
    let (r3, mut rx3) = TestRuntime::channel::<LifecycleCommand>(3, 4);
    group.try_add(1, r1.clone(), ChildPolicy::Stop).unwrap();
    group.try_add(2, r2, ChildPolicy::Stop).unwrap();
    group.try_add(3, r3, ChildPolicy::Stop).unwrap();
    group.handle_started(1, FROM);
    group.handle_started(2, FROM);
    group.handle_started(3, FROM);

    drop(rx2); // child 2 task gone
    r1.try_send(FROM, LifecycleCommand::Ping).unwrap(); // fill child 1's channel

    group.stop_all(FROM);

    // Child 1: Full → queued. Child 2: Closed → Gone. Child 3: delivered.
    assert_eq!(group.children[0].pending_cmd, Some(PendingCmd::Stop));
    assert_eq!(group.children[1].phase, ChildPhase::Gone);
    assert!(matches!(rx3.drain_payloads()[..], [LifecycleCommand::Stop]));

    // Flush after drain: child 1 gets its Stop. Children 1 and 3 are still
    // non-terminal — the Stop *report* drives the phase, not the send.
    rx1.drain_payloads();
    group.flush_pending(FROM);
    assert!(matches!(rx1.drain_payloads()[..], [LifecycleCommand::Stop]));
    assert!(!group.all_stopped());

    // The Stopped reports arrive → terminal; child 2 is already Gone.
    group.record_stopped(1, FROM);
    group.record_stopped(3, FROM);
    assert!(group.all_stopped());
}

#[test]
fn start_all_skips_terminal_children() {
    let (mut group, mut rx, _notify_ref, _notify_rx) = setup_one_child(ChildPolicy::Stop);

    // Non-terminal children are started.
    group.start_all(FROM);
    assert!(matches!(rx.drain_payloads()[..], [LifecycleCommand::Start]));

    // Terminal children (Stopped here; task-gone phases alike) are skipped —
    // a supervisor-level Reset must not resurrect children whose epoch is
    // accounted as over.
    group.record_stopped(1, FROM);
    group.start_all(FROM);
    assert_eq!(rx.drain_payloads().len(), 0);
}
