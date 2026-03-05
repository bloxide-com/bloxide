// Copyright 2025 Bloxide, all rights reserved
#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod capability;
pub mod peer;
pub mod prelude;

pub use capability::SpawnCap;
pub use peer::{apply_peer_ctrl, introduce_peers, AddPeer, HasPeers, PeerCtrl, RemovePeer};

#[cfg(feature = "std")]
pub mod test_impl;
