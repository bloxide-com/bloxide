// Copyright 2025 Bloxide, all rights reserved
//! Concrete runtime-specific worker spawn function for the Tokio pool demo.
//!
//! This crate is the impl layer for `tokio-pool-demo`. It is the only place
//! that knows about concrete worker context/spec types and task spawning.

extern crate alloc;
use alloc::vec::Vec;

use bloxide_core::{
    capability::{BloxRuntime, DynamicChannelCap},
    child_management::{AbortCommand, ChildPolicy},
    lifecycle::{ChildLifecycleEvent, LifecycleCommand},
    messaging::ActorRef,
    StateMachine,
};
use bloxide_spawn::{SpawnCap, SpawnOutput};
use bloxide_tokio::{run_supervised_actor_with_abort, TokioRuntime};
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
            let (abort_ref, abort_rx) =
                <TokioRuntime as DynamicChannelCap>::channel::<AbortCommand>(worker_id, 4);

            let behavior = WorkerBehavior::<TokioRuntime>::default();
            let worker_ctx = WorkerCtx::new(pool_ref, worker_id, behavior);
            let machine =
                StateMachine::<WorkerSpec<TokioRuntime, WorkerBehavior<TokioRuntime>>>::new(
                    worker_ctx,
                );

            let notify_sender = notify.sender();
            let task_handle = <TokioRuntime as SpawnCap>::spawn(async move {
                run_supervised_actor_with_abort(
                    machine,
                    (ctrl_rx, domain_rx),
                    lifecycle_rx,
                    abort_rx,
                    worker_id,
                    notify_sender,
                )
                .await
            });

            // Convert the JoinHandle (not Clone) into a KillHandle (Clone)
            // so it can be stored in RegisterDynamicChild and cloned from
            // &Event by the supervisor's action function.
            let kill_handle = <TokioRuntime as SpawnCap>::kill_handle(task_handle);

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
                abort_ref,
                kill_handle,
                policy: ChildPolicy::Stop,
            }
        }
    }
}
