// Copyright 2025 Bloxide, all rights reserved

#![cfg_attr(feature = "runtime-embassy", no_std)]

pub mod core;
pub mod macros;
pub mod runtime;

// Core re-exports
pub use crate::{core::actor::*, core::messaging::*};

// Conditional type exports
#[cfg(feature = "runtime-tokio")]
pub mod std_exports {
    pub use std::any::Any;
    pub use std::boxed::Box;
    pub use std::fmt::Debug;
    pub use std::fmt::Formatter;
    pub use std::future::Future;
    pub use std::marker::PhantomData;
    pub use std::pin::Pin;
    pub use std::string::String;
    pub use std::sync::Arc;
    pub use std::vec::Vec;
    pub use tokio::sync::mpsc::error::TrySendError;
}

#[cfg(feature = "runtime-embassy")]
pub mod std_exports {
    extern crate alloc;
    pub use alloc::boxed::Box;
    pub use alloc::string::String;
    pub use alloc::vec::Vec;
    pub use core::any::Any;
    pub use core::fmt::Debug;
    pub use core::fmt::Formatter;
    pub use core::future::Future;
    pub use core::marker::PhantomData;
    pub use core::pin::Pin;
    pub use embassy_sync::channel::TrySendError;
}

// Re-export everything from std_exports at crate root
pub use std_exports::*;

#[cfg(all(not(feature = "runtime-tokio"), not(feature = "runtime-embassy")))]
compile_error!("Either 'runtime-tokio' or 'runtime-embassy' feature must be enabled");

#[cfg(all(feature = "runtime-tokio", feature = "runtime-embassy"))]
compile_error!("Only one runtime can be enabled at a time");
