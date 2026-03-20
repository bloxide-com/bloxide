// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::{messaging::ActorId, KillCap};
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
    pub fn register(&self, actor_id: ActorId, handle: JoinHandle<()>) {
        self.tasks.lock().unwrap().insert(actor_id, handle);
    }

    /// Remove an actor from tracking (e.g., when it completes normally).
    pub fn unregister(&self, actor_id: ActorId) {
        self.tasks.lock().unwrap().remove(&actor_id);
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

        // Give a moment for abort to process
        sleep(Duration::from_millis(10)).await;

        // Task should be removed from tracking
        assert!(kill_cap.tasks.lock().unwrap().get(&actor_id).is_none());
    }

    #[tokio::test]
    async fn kill_nonexistent_is_noop() {
        let kill_cap = TokioKillCap::new();
        // Should not panic
        kill_cap.kill(999);
    }
}
