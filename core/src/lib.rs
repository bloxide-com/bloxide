// Copyright 2025 Bloxide, all rights reserved
#![no_std]

pub mod blox;
pub mod components;
pub mod macros;
pub mod merge;
pub mod messaging;
pub mod state_machine;

pub mod prelude {
    pub use crate::{components::*, messaging::*, state_machine::*};
    extern crate alloc;
    pub use alloc::{boxed::Box, string::String, vec::Vec};
    pub use core::{
        any::Any,
        cell::{LazyCell, OnceCell},
        fmt,
        future::Future,
        hash::{Hash, Hasher},
        marker::PhantomData,
        pin::Pin,
    };
    pub use hashbrown::HashMap;
}
