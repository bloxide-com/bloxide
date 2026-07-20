// Copyright 2025 Bloxide, all rights reserved
use core::future::Future;

use crate::TokioRuntime;

impl bloxide_core::SpawnCap for TokioRuntime {
    type TaskHandle = tokio::task::JoinHandle<()>;
    type AbortHandle = tokio::task::AbortHandle;

    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self::TaskHandle {
        tokio::spawn(future)
    }

    fn abort_handle(handle: Self::TaskHandle) -> Self::AbortHandle {
        handle.abort_handle()
    }

    fn abort(handle: Self::AbortHandle) {
        handle.abort();
    }
}
