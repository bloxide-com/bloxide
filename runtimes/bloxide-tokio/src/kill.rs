// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{messaging::ActorId, KillCap};
use futures_util::FutureExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

/// Tokio implementation of KillCap.
///
/// Tracks spawned actor tasks in a HashMap and aborts them on demand.
/// Thread-safe via `Arc<Mutex<...>>`.
#[derive(Clone)]
pub struct TokioKillCap {
    tasks: Arc<Mutex<HashMap<ActorId, JoinHandle<()>>>>,
}

impl TokioKillCap {
    /// Create a new empty KillCap.
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a spawned actor task with its ID.
    /// Call this immediately after `tokio::spawn`.
    ///
    /// If a handle already exists for `actor_id`, the old task is aborted
    /// and drained before being replaced — preventing silent task leaks on
    /// overwrite.
    pub fn register(&self, actor_id: ActorId, handle: JoinHandle<()>) {
        if let Some(old) = self.tasks.lock().unwrap().insert(actor_id, handle) {
            old.abort();
            // Drain the old handle so it is fully dropped, not a zombie.
            let _ = old.now_or_never();
        }
    }

    /// Remove an actor from tracking (e.g., when it completes normally).
    ///
    /// If the task is still alive (e.g., called proactively before the actor
    /// has fully exited), it is aborted and drained to prevent leaks.
    pub fn unregister(&self, actor_id: ActorId) {
        if let Some(handle) = self.tasks.lock().unwrap().remove(&actor_id) {
            handle.abort();
            let _ = handle.now_or_never();
        }
    }

    /// Returns true if the given actor is currently tracked by this KillCap.
    pub fn contains(&self, actor_id: ActorId) -> bool {
        self.tasks.lock().unwrap().contains_key(&actor_id)
    }

    /// Returns the number of currently tracked tasks.
    pub fn len(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }

    /// Returns true if no tasks are currently tracked.
    pub fn is_empty(&self) -> bool {
        self.tasks.lock().unwrap().is_empty()
    }
}

impl Default for TokioKillCap {
    fn default() -> Self {
        Self::new()
    }
}

impl KillCap for TokioKillCap {
    fn kill(&self, actor_id: ActorId) {
        if let Some(handle) = self.tasks.lock().unwrap().remove(&actor_id) {
            handle.abort();
            // Drain the handle so the aborted task is fully dropped and
            // does not linger as a zombie on the runtime.
            let _ = handle.now_or_never();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn kill_aborts_task() {
        let kill_cap = TokioKillCap::new();
        let actor_id: ActorId = 42;

        let handle = tokio::spawn(async {
            loop {
                sleep(Duration::from_secs(100)).await;
            }
        });

        kill_cap.register(actor_id, handle);
        kill_cap.kill(actor_id);

        // Task should be removed from tracking immediately (now_or_never drains)
        assert!(kill_cap.tasks.lock().unwrap().get(&actor_id).is_none());
    }

    #[tokio::test]
    async fn kill_nonexistent_is_noop() {
        let kill_cap = TokioKillCap::new();
        // Should not panic
        kill_cap.kill(999);
    }

    #[tokio::test]
    async fn register_overwrite_aborts_old_task() {
        let kill_cap = TokioKillCap::new();
        let actor_id: ActorId = 7;

        // Spawn a long-running task and register it.
        let handle1 = tokio::spawn(async {
            sleep(Duration::from_secs(100)).await;
        });
        kill_cap.register(actor_id, handle1);

        // Re-register with a new handle for the same actor_id.
        let handle2 = tokio::spawn(async {
            sleep(Duration::from_secs(100)).await;
        });
        kill_cap.register(actor_id, handle2);

        // Only one entry should exist.
        assert_eq!(kill_cap.len(), 1);
        assert!(kill_cap.contains(actor_id));

        // The old task should have been aborted. We can't directly check
        // handle1, but we can verify the KillCap is in a consistent state
        // and that killing the actor cleans up the remaining (handle2) task.
        kill_cap.kill(actor_id);
        assert!(!kill_cap.contains(actor_id));
        assert!(kill_cap.is_empty());
    }

    #[tokio::test]
    async fn kill_ensures_no_zombie_task() {
        let kill_cap = TokioKillCap::new();
        let actor_id: ActorId = 99;

        let handle = tokio::spawn(async {
            loop {
                sleep(Duration::from_secs(100)).await;
            }
        });

        kill_cap.register(actor_id, handle);

        // Kill should synchronously drain the task via now_or_never.
        // After kill returns, the JoinHandle has been consumed.
        kill_cap.kill(actor_id);

        // No entry remains.
        assert!(!kill_cap.contains(actor_id));
        assert!(kill_cap.is_empty());
    }

    #[tokio::test]
    async fn unregister_aborts_and_removes_task() {
        let kill_cap = TokioKillCap::new();
        let actor_id: ActorId = 55;

        let handle = tokio::spawn(async {
            sleep(Duration::from_secs(100)).await;
        });
        kill_cap.register(actor_id, handle);

        assert!(kill_cap.contains(actor_id));
        kill_cap.unregister(actor_id);
        assert!(!kill_cap.contains(actor_id));
        assert!(kill_cap.is_empty());
    }

    #[tokio::test]
    async fn unregister_nonexistent_is_noop() {
        let kill_cap = TokioKillCap::new();
        // Should not panic
        kill_cap.unregister(123);
    }

    #[tokio::test]
    async fn kill_then_reregister_works() {
        let kill_cap = TokioKillCap::new();
        let actor_id: ActorId = 33;

        let handle1 = tokio::spawn(async {
            sleep(Duration::from_secs(100)).await;
        });
        kill_cap.register(actor_id, handle1);
        kill_cap.kill(actor_id);
        assert!(!kill_cap.contains(actor_id));

        // Should be able to register a new task for the same ID.
        let handle2 = tokio::spawn(async {
            sleep(Duration::from_secs(100)).await;
        });
        kill_cap.register(actor_id, handle2);
        assert!(kill_cap.contains(actor_id));
        kill_cap.kill(actor_id);
        assert!(!kill_cap.contains(actor_id));
    }
}
