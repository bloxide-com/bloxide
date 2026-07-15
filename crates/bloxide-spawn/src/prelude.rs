// Copyright 2025 Bloxide, all rights reserved
//! Convenience re-exports for wiring sites.

pub use crate::capability::SpawnCap;
pub use crate::factory::{
    ErasedSpawnFactory, FactoryWrapper, SpawnCapability, SpawnFactoryFor,
};
pub use crate::output::{SpawnOutput, SpawnPolicy};
pub use crate::peer::{
    introduce_peers, AddPeer, HasPeers, PeerCtrl, RemovePeer,
};
