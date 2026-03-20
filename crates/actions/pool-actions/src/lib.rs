// Copyright 2025 Bloxide, all rights reserved
//! Domain action traits and generic functions for the worker pool example.
//!
//! This crate is a portable action crate: traits and trait-bounded generic
//! functions only. No concrete runtime types or runtime-specific imports.
#![no_std]
extern crate alloc;

pub mod actions;
pub mod traits;

pub mod prelude {
    pub use crate::actions::*;
    pub use crate::traits::*;
}
