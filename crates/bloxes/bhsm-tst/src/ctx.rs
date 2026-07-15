// Copyright 2025 Bloxide, all rights reserved
use bloxide_core::ActorId;
use bloxide_macros::BloxCtx;

/// Context for the bhsm-tst blox.
#[derive(BloxCtx)]
pub struct BhsmTstCtx {
    pub self_id: ActorId,
}
