//! Pure domain message types for the worker pool example.
//!
//! No runtime dependencies — only plain data.
#![no_std]

pub mod prelude {
    pub use crate::*;
}

/// Messages sent to the pool actor.
#[derive(Debug)]
pub enum PoolMsg {
    SpawnWorker(SpawnWorker),
    WorkDone(WorkDone),
}

/// Instruct the pool to spawn a worker for the given task.
#[derive(Debug)]
pub struct SpawnWorker {
    pub task_id: u32,
}

/// Sent by a worker to the pool when it finishes its task.
#[derive(Debug)]
pub struct WorkDone {
    /// `ActorId = usize` — no bloxide-core import needed here.
    pub worker_id: usize,
    pub task_id: u32,
    pub result: u32,
}

/// Messages sent to a worker actor.
#[derive(Debug)]
pub enum WorkerMsg {
    DoWork(DoWork),
    PeerResult(PeerResult),
}

/// Assign a task to a worker.
#[derive(Debug)]
pub struct DoWork {
    pub task_id: u32,
}

/// Broadcast result from one worker to its peers.
#[derive(Debug)]
pub struct PeerResult {
    pub from_id: usize,
    pub result: u32,
}
