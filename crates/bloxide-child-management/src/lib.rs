// Copyright 2025 Bloxide, all rights reserved
//! Child group tracking, restart strategies, and health checking.
//!
//! This is the reusable platform primitive for managing supervised children.
//! It is not supervisor-specific — any blox that tracks child actors can use
//! `ChildGroup` directly. The supervisor is one such consumer; a custom
//! managing blox could use it without depending on `bloxide-supervisor`.

#![no_std]
extern crate alloc;

use alloc::vec::Vec;
use bloxide_core::{
    capability::{BloxRuntime, KillCapability},
    child_management::AbortCommand,
    lifecycle::LifecycleCommand,
    messaging::{ActorId, ActorRef},
};

// Re-export child-management policies from bloxide-core for convenience.
pub use bloxide_core::child_management::{ChildPolicy, GroupShutdown, RestartStrategy};

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

    pub fn handle_done_or_failed(&mut self, child_id: ActorId, from: ActorId) -> ChildAction {
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
            // For now, mark as permanently done since the task is ending.
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

    pub fn health_check_tick(&mut self, from: ActorId) -> ChildAction {
        let stale_ids: Vec<ActorId> = self
            .children
            .iter()
            .filter(|entry| Self::is_health_monitored(entry) && entry.awaiting_alive)
            .map(|entry| entry.id)
            .collect();

        let mut action = ChildAction::Continue;
        for child_id in stale_ids {
            if self.handle_done_or_failed(child_id, from) == ChildAction::BeginShutdown {
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

    pub fn all_stopped(&self) -> bool {
        self.stopped_count == self.children.len()
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
    use bloxide_test_runtime::{TestRuntime, TestReceiver};

    fn setup_one_child(
        policy: ChildPolicy,
    ) -> (
        ChildGroup<TestRuntime>,
        TestReceiver<LifecycleCommand>,
    ) {
        let mut group = ChildGroup::new(GroupShutdown::WhenAnyDone);
        let id = 1usize;
        let (lifecycle_ref, rx) = TestRuntime::channel::<LifecycleCommand>(id, 16);
        group.add(id, lifecycle_ref, policy);
        (group, rx)
    }

    #[test]
    fn duplicate_done_while_awaiting_restart_is_coalesced() {
        let (mut group, mut rx) = setup_one_child(ChildPolicy::Restart { max: 2 });
        let from = 100usize;

        // First Done → triggers Reset
        let action = group.handle_done_or_failed(1, from);
        assert_eq!(action, ChildAction::Continue);
        assert_eq!(rx.drain_payloads().len(), 1); // Reset sent

        // Second Done while ResetPending → coalesced (no second Reset)
        let action = group.handle_done_or_failed(1, from);
        assert_eq!(action, ChildAction::Continue);
        assert_eq!(rx.drain_payloads().len(), 0); // nothing sent
    }

    #[test]
    fn health_tick_pings_child_and_marks_missed_alive_as_failed() {
        let (mut group, mut rx) = setup_one_child(ChildPolicy::Restart { max: 1 });
        let from = 100usize;
        // Start the child first
        group.handle_started(1);
        // Health check tick should ping the child
        group.health_check_tick(from);
        let cmds = rx.drain_payloads();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], LifecycleCommand::Ping));
    }
}
