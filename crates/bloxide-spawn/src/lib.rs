// Copyright 2025 Bloxide, all rights reserved
#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod capability;
pub mod factory;
pub mod output;
pub mod peer;
pub mod prelude;

#[cfg(feature = "std")]
pub mod test_impl;

pub use capability::SpawnCap;
pub use factory::{ErasedSpawnFactory, FactoryWrapper, SpawnCapability, SpawnFactoryFor};
pub use output::{SpawnOutput, SpawnPolicy};
pub use peer::{introduce_peers, AddPeer, HasPeers, PeerCtrl, RemovePeer};

#[cfg(all(test, feature = "std"))]
mod tests;
