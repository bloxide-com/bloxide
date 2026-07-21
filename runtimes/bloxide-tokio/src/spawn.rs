// Copyright 2025 Bloxide, all rights reserved
use core::future::Future;

use crate::TokioRuntime;

impl bloxide_spawn::SpawnCap for TokioRuntime {
    type TaskHandle = tokio::task::JoinHandle<()>;
    type KillHandle = tokio::task::AbortHandle;

    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self::TaskHandle {
        tokio::spawn(future)
    }

    fn kill_handle(handle: Self::TaskHandle) -> Self::KillHandle {
        handle.abort_handle()
    }

    fn kill(handle: Self::KillHandle) {
        handle.abort();
    }
}
