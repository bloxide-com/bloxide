// Copyright 2025 Bloxide, all rights reserved
//! Concrete runtime-specific worker spawn function for the Tokio pool demo.
//!
//! This crate is the impl layer for `tokio-pool-demo`. It is the only place
//! that knows about concrete worker context/spec types and task spawning.
//!
//! Per spec 22 Step 5, the old `AppSpawnFactory` struct was replaced with a
//! stateless `fn spawn_worker`. All state comes from the `SpawnRequest`
//! message — no captured struct fields.

extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap, SpawnCap},
    child_management::{ChildPolicy, KillCommand},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::ActorRef,
    spawn::SpawnOutput,
    StateMachine,
};
use bloxide_tokio::{run_supervised_actor_with_kill, TokioRuntime};
use pool_actions::traits::{HasCurrentTask, HasWorkerPeers};
use pool_messages::{SpawnRequest, SpawnedWorker, WorkerCtrl, WorkerMsg};
use worker_blox::{WorkerCtx, WorkerSpec};

/// Behavior type for Worker actors holding task state and peer list.
pub struct WorkerBehavior<R: BloxRuntime> {
    task_id: u32,
    result: u32,
    peers: Vec<ActorRef<WorkerMsg, R>>,
}

impl<R: BloxRuntime> Default for WorkerBehavior<R> {
    fn default() -> Self {
        Self {
            task_id: 0,
            result: 0,
            peers: Vec::new(),
        }
    }
}

impl<R: BloxRuntime> HasCurrentTask for WorkerBehavior<R> {
    fn task_id(&self) -> u32 {
        self.task_id
    }
    fn set_task_id(&mut self, id: u32) {
        self.task_id = id;
    }
    fn result(&self) -> u32 {
        self.result
    }
    fn set_result(&mut self, r: u32) {
        self.result = r;
    }
}

impl<R: BloxRuntime> HasWorkerPeers<R> for WorkerBehavior<R> {
    fn peers(&self) -> &[ActorRef<WorkerMsg, R>] {
        &self.peers
    }
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>> {
        &mut self.peers
    }
}

/// Spawn function for the Tokio pool demo.
///
/// Creates a worker actor and returns the handles the supervisor needs.
/// This is a plain function (not a trait impl) — the wiring layer passes
/// it to `spawn_child()` as a `SpawnFn<R, SpawnRequest<R>>`.
///
/// All state comes from the request — `pool_ref` is in the message, not
/// captured from a struct field.
pub fn spawn_worker(
    req: SpawnRequest<TokioRuntime>,
    notify: ActorRef<ChildLifecycleEvent, TokioRuntime>,
) -> SpawnOutput<TokioRuntime> {
    match req {
        SpawnRequest::Worker {
            task_id: _,
            reply_to,
            pool_ref,
        } => {
            let worker_id = <TokioRuntime as DynamicChannelCap>::alloc_actor_id();
            let (ctrl_ref, ctrl_rx) = <TokioRuntime as DynamicChannelCap>::channel::<
                WorkerCtrl<TokioRuntime>,
            >(worker_id, 16);
            let (domain_ref, domain_rx) =
                <TokioRuntime as DynamicChannelCap>::channel::<WorkerMsg>(worker_id, 16);
            let (lifecycle_ref, lifecycle_rx) =
                <TokioRuntime as DynamicChannelCap>::channel::<LifecycleCommand>(worker_id, 4);
            let (kill_ref, kill_rx) =
                <TokioRuntime as DynamicChannelCap>::channel::<KillCommand>(worker_id, 4);

            let behavior = WorkerBehavior::<TokioRuntime>::default();
            let worker_ctx = WorkerCtx::new(pool_ref, worker_id, behavior);
            let machine =
                StateMachine::<WorkerSpec<TokioRuntime, WorkerBehavior<TokioRuntime>>>::new(
                    worker_ctx,
                );

            let notify_sender = notify.sender();
            let task_handle = <TokioRuntime as SpawnCap>::spawn(async move {
                run_supervised_actor_with_kill(
                    machine,
                    (ctrl_rx, domain_rx),
                    lifecycle_rx,
                    kill_rx,
                    worker_id,
                    notify_sender,
                )
                .await
            });

            // Convert the JoinHandle (not Clone) into an AbortHandle (Clone)
            // so it can be stored in RegisterDynamicChild and cloned from
            // &Event by the supervisor's action function.
            let abort_handle = <TokioRuntime as SpawnCap>::abort_handle(task_handle);

            let _ = reply_to.try_send(
                worker_id,
                SpawnedWorker {
                    child_id: worker_id,
                    domain_ref: domain_ref.clone(),
                    ctrl_ref: ctrl_ref.clone(),
                },
            );

            SpawnOutput {
                child_id: worker_id,
                lifecycle_ref,
                kill_ref,
                abort_handle,
                policy: ChildPolicy::Stop,
            }
        }
    }
}
