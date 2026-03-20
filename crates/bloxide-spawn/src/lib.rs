// Copyright 2025 Bloxide, all rights reserved
#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod capability;
pub mod prelude;

pub use capability::SpawnCap;

#[cfg(feature = "std")]
pub mod test_impl;
