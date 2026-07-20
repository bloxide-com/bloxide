// Copyright 2025 Bloxide, all rights reserved
use core::future::Future;

use bloxide_core::SpawnCap;

use crate::TokioRuntime;

impl SpawnCap for TokioRuntime {
    type TaskHandle = tokio::task::JoinHandle<()>;

    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self::TaskHandle {
        tokio::spawn(future)
    }

    fn kill(handle: Self::TaskHandle) {
        handle.abort();
    }
}
