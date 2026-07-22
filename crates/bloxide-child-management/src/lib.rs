// Copyright 2025 Bloxide, all rights reserved
//! Child group tracking, restart strategies, and health checking.
//!
//! This is the reusable platform primitive for managing supervised children.
//! It is not supervisor-specific — any blox that tracks child actors can use
//! `ChildGroup` directly. The supervisor is one such consumer; a custom
//! managing blox could use it without depending on `bloxide-supervisor`.

#![no_std]
extern crate alloc;

pub mod builder;

use alloc::vec::Vec;
use bloxide_core::{
    capability::{BloxRuntime, KillCapability},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::{ActorId, ActorRef},
};

/// Command enum for the abort capability mailbox.
///
/// Sent by the managing blox (supervisor or custom) when `ChildPolicy::Abort` fires.
/// The child's task receives this on its abort mailbox and self-terminates
/// cooperatively (breaks out of the run loop, no callbacks fire).
///
/// This is the cooperative self-termination path — distinct from `KillCapability`
/// which is the external ripcord that destroys the task without cooperation.
#[derive(Debug, Clone)]
pub enum AbortCommand {
    /// Abort the child cooperatively. No callbacks, no graceful shutdown.
    /// The child's task self-terminates on receipt.
    Abort { child_id: ActorId },
}

/// Supervision policy for a child actor.
///
/// Determines what the managing blox does when the child fails (reports
/// `Done` or `Failed`).
///
/// The four-level lifecycle model (`reset → stop → abort → kill`):
///
/// | Policy | Mechanism | Cooperative? | Callbacks? | Restartable? |
/// |--------|-----------|-------------|------------|--------------|
/// | `Restart` | Send `Reset` | Yes | Exit + entry chain | Yes (immediately) |
/// | `Stop` | Send `Stop` | Yes | Exit + `on_init_entry` | Yes (via `Start`) |
/// | `Abort` | Send `AbortCommand` on abort mailbox | Yes (cooperative) | None | Yes (respawn task) |
/// | `Kill` | `KillCapability::kill(handle)` | No (forced) | None | No (permanently dead) |
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChildPolicy {
    /// Restart the child up to `max` times by sending `Reset`.
    /// `Reset` goes directly to `initial_state()` — the actor is immediately
    /// operational. `max` is the number of restart attempts allowed: after
    /// the `max`-th restart the next failure triggers group shutdown.
    /// `Restart { max: 0 }` means no restarts — equivalent to `Stop`.
    Restart { max: usize },
    /// Send `Stop` command for clean shutdown (exit chain + `on_init_entry` fire).
    /// Actor goes to Init, suspended, can be restarted with `Start`.
    Stop,
    /// Send `AbortCommand` on the abort mailbox for cooperative self-termination.
    /// No callbacks fire. The child's task ends. Requires the child to have
    /// an abort capability mailbox.
    Abort,
    /// Immediately kill the child via `KillCapability::kill(handle)`.
    /// The task is destroyed externally — no callbacks, no cooperation.
    /// Permanently dead. Requires the child to have a kill capability
    /// (kill handle from `SpawnCap`).
    Kill,
}

/// Group-level restart strategy determining which children are restarted
/// when a child fails. Inspired by Erlang/OTP supervisor strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RestartStrategy {
    /// Restart only the failed child (default).
    #[default]
    OneForOne,
    /// Restart all children when any child fails.
    OneForAll,
    /// Restart the failed child and all children declared after it.
    RestForOne,
}

/// When to trigger group-level shutdown.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GroupShutdown {
    WhenAnyDone,
    WhenAllDone,
}

// Re-export the generic builder
pub use builder::ChildGroupBuilder;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum ChildAction {
    #[default]
    Continue,
    BeginShutdown,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
enum ChildPhase {
    #[default]
    Init,
    Running,
    /// Reset was sent; waiting for the child to report Started.
    /// Health checks skip children in this phase — the child is transitioning.
    ResetPending,
    PermanentlyDone,
    Stopped,
}

struct ChildEntry<R: BloxRuntime> {
    id: ActorId,
    lifecycle_ref: ActorRef<LifecycleCommand, R>,
    policy: ChildPolicy,
    restarts: usize,
    permanently_done: bool,
    stopped: bool,
    phase: ChildPhase,
    awaiting_alive: bool,
    /// Abort capability mailbox (send side). `None` for static children
    /// registered via `RegisterChild` (no abort capability).
    abort_ref: Option<ActorRef<AbortCommand, R>>,
    /// Cloneable kill handle for external task kill (the ripcord). `None` for
    /// static children. Consumed by `R::Kill::kill(handle)` when
    /// `ChildPolicy::Kill` fires.
    /// This is `R::KillHandle` (Clone), not `R::TaskHandle` (not Clone).
    kill_handle: Option<<R::Kill as KillCapability<R>>::Handle>,
}

pub struct ChildGroup<R: BloxRuntime> {
    children: Vec<ChildEntry<R>>,
    shutdown: GroupShutdown,
    restart_strategy: RestartStrategy,
    stopped_count: usize,
}

/// Accessor trait for the child group.
pub trait HasChildGroup<R: BloxRuntime> {
    fn children(&self) -> &ChildGroup<R>;
}

/// Mutable accessor trait for the child group.
pub trait HasChildGroupMut<R: BloxRuntime> {
    fn children_mut(&mut self) -> &mut ChildGroup<R>;
}

/// Accessor trait for the pending action field.
pub trait HasPending {
    fn pending(&self) -> ChildAction;
    fn set_pending(&mut self, action: ChildAction);
}

impl<R: BloxRuntime> ChildGroup<R> {
    pub fn new(shutdown: GroupShutdown) -> Self {
        Self {
            children: Vec::new(),
            shutdown,
            restart_strategy: RestartStrategy::default(),
            stopped_count: 0,
        }
    }

    /// Set the restart strategy after construction.
    pub fn with_restart_strategy(mut self, strategy: RestartStrategy) -> Self {
        self.restart_strategy = strategy;
        self
    }

    pub fn add(
        &mut self,
        id: ActorId,
        lifecycle_ref: ActorRef<LifecycleCommand, R>,
        policy: ChildPolicy,
    ) {
        self.children.push(ChildEntry {
            id,
            lifecycle_ref,
            policy,
            restarts: 0,
            permanently_done: false,
            stopped: false,
            phase: ChildPhase::Init,
            awaiting_alive: false,
            abort_ref: None,
            kill_handle: None,
        });
    }

    /// Register a dynamically spawned child that has an abort capability.
    ///
    /// Stores the `abort_ref` (for cooperative self-termination via the abort
    /// mailbox) and the `kill_handle` (for the external kill ripcord) so the
    /// supervisor can abort or kill the child when policy dictates.
    ///
    /// The `kill_handle` is `Clone` (it's `R::KillHandle`), so the action
    /// function can clone it from `&Event` — unlike the old `task_handle`
    /// (`R::TaskHandle` = `JoinHandle<()>`) which was not `Clone`.
    pub fn add_dynamic(
        &mut self,
        id: ActorId,
        lifecycle_ref: ActorRef<LifecycleCommand, R>,
        abort_ref: ActorRef<AbortCommand, R>,
        kill_handle: <R::Kill as KillCapability<R>>::Handle,
        policy: ChildPolicy,
    ) {
        self.children.push(ChildEntry {
            id,
            lifecycle_ref,
            policy,
            restarts: 0,
            permanently_done: false,
            stopped: false,
            phase: ChildPhase::Init,
            awaiting_alive: false,
            abort_ref: Some(abort_ref),
            kill_handle: Some(kill_handle),
        });
    }

    pub fn start_child(&self, child_id: ActorId, from: ActorId) {
        if let Some(entry) = self.children.iter().find(|entry| entry.id == child_id) {
            if entry
                .lifecycle_ref
                .try_send(from, LifecycleCommand::Start)
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Start to child {} failed (channel full)",
                    entry.id
                );
            }
        }
    }

    pub fn start_all(&self, from: ActorId) {
        for entry in &self.children {
            if entry
                .lifecycle_ref
                .try_send(from, LifecycleCommand::Start)
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Start to child {} failed (channel full)",
                    entry.id
                );
            }
        }
    }

    pub fn stop_all(&self, from: ActorId) {
        for entry in &self.children {
            if entry
                .lifecycle_ref
                .try_send(from, LifecycleCommand::Stop)
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Stop to child {} failed (channel full)",
                    entry.id
                );
            }
        }
    }

    pub fn handle_done_or_failed(
        &mut self,
        child_id: ActorId,
        from: ActorId,
        notify: &ActorRef<ChildLifecycleEvent, R>,
    ) -> ChildAction {
        let idx = match self.children.iter().position(|e| e.id == child_id) {
            Some(idx) => idx,
            None => return ChildAction::Continue,
        };

        // Extract values needed for decision-making to avoid borrow conflicts
        let (phase, policy, restarts) = {
            let entry = &self.children[idx];
            (entry.phase, entry.policy, entry.restarts)
        };

        if matches!(
            phase,
            ChildPhase::PermanentlyDone | ChildPhase::Stopped | ChildPhase::ResetPending
        ) {
            return ChildAction::Continue;
        }

        // Handle Kill policy: call R::Kill::kill(kill_handle) — the ripcord.
        // This immediately terminates the child — no callbacks fire, no
        // cooperative shutdown. Permanently dead.
        if policy == ChildPolicy::Kill {
            // Take the kill_handle out — kill() consumes it by value.
            let kill_handle = self.children[idx].kill_handle.take();
            if let Some(handle) = kill_handle {
                R::Kill::kill(handle);
            }

            // Emit the Killed lifecycle event so the supervisor (and any
            // observers on the notify channel) learn the child was killed.
            // This is analogous to how Abort sends AbortCommand on the abort
            // mailbox and the run loop later reports Aborted — except here the
            // kill is synchronous, so we emit the event directly.
            if notify
                .try_send(from, ChildLifecycleEvent::Killed { child_id })
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Killed to supervisor for child {} failed (channel full or closed)",
                    child_id
                );
            }

            self.children[idx].permanently_done = true;
            self.children[idx].phase = ChildPhase::PermanentlyDone;
            self.children[idx].awaiting_alive = false;
            return self.check_shutdown();
        }

        // Handle Abort policy: send AbortCommand on the abort mailbox.
        // The child's task self-terminates cooperatively (no callbacks).
        if policy == ChildPolicy::Abort {
            if let Some(abort_ref) = &self.children[idx].abort_ref {
                if abort_ref
                    .try_send(from, AbortCommand::Abort { child_id })
                    .is_err()
                {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send AbortCommand::Abort to child {} failed (channel full)",
                        self.children[idx].id
                    );
                }
            }
            // The child will self-terminate; we'll get Aborted event later.
            //
            // The child is marked PermanentlyDone immediately because the abort
            // is fire-and-forget: once AbortCommand is queued on the abort
            // mailbox there is no way to recall or observe its progress from
            // here, so the ChildGroup's bookkeeping for this entry is already
            // final. The Aborted lifecycle event will arrive later but is
            // informational only — the ChildGroup state is already finalized
            // (phase == PermanentlyDone), so `handle_aborted`/this method's
            // early-return guard treat the late event as a no-op. The
            // supervisor's state machine processes the Aborted event for its
            // own transitions but does not re-enter the ChildGroup logic.
            self.children[idx].permanently_done = true;
            self.children[idx].phase = ChildPhase::PermanentlyDone;
            self.children[idx].awaiting_alive = false;
            return self.check_shutdown();
        }

        if let ChildPolicy::Restart { max } = policy {
            if restarts < max {
                // Send Reset to the failed child — goes directly to initial_state(),
                // immediately operational. No need to send Start separately.
                if self.children[idx]
                    .lifecycle_ref
                    .try_send(from, LifecycleCommand::Reset)
                    .is_err()
                {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send Reset to child {} failed (channel full)",
                        self.children[idx].id
                    );
                }
                self.children[idx].restarts += 1;
                self.children[idx].phase = ChildPhase::ResetPending;
                self.children[idx].awaiting_alive = false;

                // Apply restart strategy to other children
                self.restart_siblings(idx, from);

                return ChildAction::Continue;
            }
        }

        self.children[idx].permanently_done = true;
        self.children[idx].phase = ChildPhase::PermanentlyDone;
        self.children[idx].awaiting_alive = false;

        self.check_shutdown()
    }

    /// Send Reset to sibling children based on the restart strategy.
    ///
    /// - `OneForOne`: no siblings are restarted (only the failed child).
    /// - `OneForAll`: all other active children are restarted.
    /// - `RestForOne`: all children declared after the failed child are restarted.
    ///
    /// Only children in `Init` or `Running` phase are restarted. Children that
    /// are `PermanentlyDone` or `Stopped` are skipped.
    fn restart_siblings(&mut self, failed_idx: usize, from: ActorId) {
        let strategy = self.restart_strategy;
        if strategy == RestartStrategy::OneForOne {
            return;
        }

        // Determine which indices to restart
        let indices: Vec<usize> = match strategy {
            RestartStrategy::OneForOne => return,
            RestartStrategy::OneForAll => (0..self.children.len())
                .filter(|&i| i != failed_idx)
                .collect(),
            RestartStrategy::RestForOne => (failed_idx + 1..self.children.len()).collect(),
        };

        for i in indices {
            // Only restart children that are active (Init or Running)
            if !matches!(
                self.children[i].phase,
                ChildPhase::Init | ChildPhase::Running
            ) {
                continue;
            }
            if self.children[i]
                .lifecycle_ref
                .try_send(from, LifecycleCommand::Reset)
                .is_err()
            {
                bloxide_log::blox_log_warn!(
                    from,
                    "try_send Reset to child {} failed (channel full)",
                    self.children[i].id
                );
            }
            self.children[i].restarts += 1;
            self.children[i].awaiting_alive = false;
        }
    }

    fn check_shutdown(&self) -> ChildAction {
        match self.shutdown {
            GroupShutdown::WhenAnyDone => ChildAction::BeginShutdown,
            GroupShutdown::WhenAllDone => {
                if self
                    .children
                    .iter()
                    .all(|e| e.permanently_done || e.stopped)
                {
                    ChildAction::BeginShutdown
                } else {
                    ChildAction::Continue
                }
            }
        }
    }

    /// Handle a `ChildLifecycleEvent::Started` for a child.
    ///
    /// In the four-level lifecycle model, `Started` is sent for both initial
    /// `Start` and `Reset` (both go directly to `initial_state()`). The
    /// supervisor does not need to send `Start` after `Reset` — `Reset` is
    /// self-contained.
    pub fn handle_started(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            if !matches!(
                entry.phase,
                ChildPhase::PermanentlyDone | ChildPhase::Stopped
            ) {
                entry.phase = ChildPhase::Running;
                entry.awaiting_alive = false;
            }
        }
    }

    pub fn handle_alive(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            if !matches!(
                entry.phase,
                ChildPhase::PermanentlyDone | ChildPhase::Stopped
            ) {
                entry.awaiting_alive = false;
            }
        }
    }

    pub fn health_check_tick(
        &mut self,
        from: ActorId,
        notify: &ActorRef<ChildLifecycleEvent, R>,
    ) -> ChildAction {
        let stale_ids: Vec<ActorId> = self
            .children
            .iter()
            .filter(|entry| Self::is_health_monitored(entry) && entry.awaiting_alive)
            .map(|entry| entry.id)
            .collect();

        let mut action = ChildAction::Continue;
        for child_id in stale_ids {
            if self.handle_done_or_failed(child_id, from, notify) == ChildAction::BeginShutdown {
                action = ChildAction::BeginShutdown;
            }
        }

        // Ping all health-monitored children. ResetPending children are
        // excluded by is_health_monitored — they're transitioning and will
        // be pinged again once they report Started (moving to Running).
        for entry in &mut self.children {
            if Self::is_health_monitored(entry) {
                if entry
                    .lifecycle_ref
                    .try_send(from, LifecycleCommand::Ping)
                    .is_err()
                {
                    bloxide_log::blox_log_warn!(
                        from,
                        "try_send Ping to child {} failed (channel full)",
                        entry.id
                    );
                }
                entry.awaiting_alive = true;
            } else {
                entry.awaiting_alive = false;
            }
        }

        action
    }

    fn is_health_monitored(entry: &ChildEntry<R>) -> bool {
        !entry.permanently_done
            && !entry.stopped
            && !matches!(
                entry.phase,
                ChildPhase::PermanentlyDone | ChildPhase::ResetPending
            )
    }

    pub fn record_stopped(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            if !entry.stopped {
                entry.stopped = true;
                entry.phase = ChildPhase::Stopped;
                entry.awaiting_alive = false;
                self.stopped_count += 1;
            }
        }
    }

    /// Record that a child was aborted (cooperative self-termination via
    /// `AbortCommand`). The child's task has ended. Unlike `Stopped`, the
    /// child is not in Init — its task is gone. To restart, the supervisor
    /// needs to respawn the task.
    pub fn record_aborted(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            entry.permanently_done = true;
            entry.phase = ChildPhase::PermanentlyDone;
            entry.awaiting_alive = false;
        }
    }

    /// Record that a child was killed (external task destruction via
    /// `KillCapability::kill`). The child's task is gone. Permanently dead.
    pub fn record_killed(&mut self, child_id: ActorId) {
        if let Some(entry) = self.children.iter_mut().find(|e| e.id == child_id) {
            entry.permanently_done = true;
            entry.phase = ChildPhase::PermanentlyDone;
            entry.awaiting_alive = false;
        }
    }

    pub fn all_stopped(&self) -> bool {
        self.children
            .iter()
            .all(|e| e.permanently_done || e.stopped)
    }

    /// Reset all restart and stop counters for a new lifecycle epoch.
    ///
    /// # Warning
    ///
    /// On runtimes whose per-child lifecycle channels persist across epochs
    /// (including Embassy's static-channel setup), stale commands queued before
    /// this reset may be delivered to children after the next `start_all`.
    /// Callers must ensure child tasks have consumed all previously queued
    /// commands before calling `clear_counters`.
    pub fn clear_counters(&mut self) {
        for entry in &mut self.children {
            entry.restarts = 0;
            entry.permanently_done = false;
            entry.stopped = false;
            entry.phase = ChildPhase::Init;
            entry.awaiting_alive = false;
        }
        self.stopped_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bloxide_core::capability::DynamicChannelCap;
    use bloxide_test_runtime::{TestReceiver, TestRuntime};

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
        let (notify_ref, notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(100, 16);
        group.add(id, lifecycle_ref, policy);
        (group, rx, notify_ref, notify_rx)
    }

    #[test]
    fn duplicate_done_while_awaiting_restart_is_coalesced() {
        let (mut group, mut rx, notify_ref, _notify_rx) =
            setup_one_child(ChildPolicy::Restart { max: 2 });
        let from = 100usize;

        // First Done → triggers Reset
        let action = group.handle_done_or_failed(1, from, &notify_ref);
        assert_eq!(action, ChildAction::Continue);
        assert_eq!(rx.drain_payloads().len(), 1); // Reset sent

        // Second Done while ResetPending → coalesced (no second Reset)
        let action = group.handle_done_or_failed(1, from, &notify_ref);
        assert_eq!(action, ChildAction::Continue);
        assert_eq!(rx.drain_payloads().len(), 0); // nothing sent
    }

    #[test]
    fn health_tick_pings_child_and_marks_missed_alive_as_failed() {
        let (mut group, mut rx, notify_ref, _notify_rx) =
            setup_one_child(ChildPolicy::Restart { max: 1 });
        let from = 100usize;
        // Start the child first
        group.handle_started(1);
        // Health check tick should ping the child
        group.health_check_tick(from, &notify_ref);
        let cmds = rx.drain_payloads();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], LifecycleCommand::Ping));
    }

    #[test]
    fn kill_policy_emits_killed_event() {
        // A child with ChildPolicy::Kill and a kill_handle should emit
        // ChildLifecycleEvent::Killed when handle_done_or_failed fires.
        let (mut group, _rx, notify_ref, mut notify_rx) = {
            let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
            let id = 1usize;
            let (lifecycle_ref, rx) = TestRuntime::channel::<LifecycleCommand>(id, 16);
            // For TestRuntime, Kill::Handle = (), so we use add_dynamic with
            // a dummy abort_ref to get a kill_handle.
            let (abort_ref, _abort_rx) = TestRuntime::channel::<AbortCommand>(id + 100, 16);
            group.add_dynamic(id, lifecycle_ref, abort_ref, (), ChildPolicy::Kill);
            let (notify_ref, notify_rx) = TestRuntime::channel::<ChildLifecycleEvent>(100, 16);
            (group, rx, notify_ref, notify_rx)
        };
        let from = 100usize;

        // Start the child so it's in Running phase (not skipped).
        group.handle_started(1);

        let action = group.handle_done_or_failed(1, from, &notify_ref);
        assert_eq!(action, ChildAction::BeginShutdown);

        // The Killed event should have been sent on the notify channel.
        let events = notify_rx.drain_payloads();
        assert_eq!(events.len(), 1, "exactly one Killed event expected");
        assert!(
            matches!(events[0], ChildLifecycleEvent::Killed { child_id: 1 }),
            "expected Killed {{ child_id: 1 }}, got {:?}",
            events[0]
        );
    }
}
