// Copyright 2025 Bloxide, all rights reserved

// Re-export runtime-specific types
#[cfg(feature = "runtime-embassy")]
pub mod embassy;
#[cfg(feature = "runtime-embassy")]
pub use embassy::*;

#[cfg(feature = "runtime-tokio")]
pub mod tokio;
#[cfg(feature = "runtime-tokio")]
pub use tokio::*;
