// Copyright 2025 Bloxide, all rights reserved
use core::future::Future;

use bloxide_spawn::SpawnCap;

use crate::TokioRuntime;

impl SpawnCap for TokioRuntime {
    fn spawn(future: impl Future<Output = ()> + Send + 'static) {
        tokio::spawn(future);
    }
}
