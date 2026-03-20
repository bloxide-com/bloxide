// Copyright 2025 Bloxide, all rights reserved
#![no_std]

pub mod prelude {
    pub use crate::*;
}

use bloxide_macros::blox_messages;

blox_messages! {
    pub enum CounterMsg {
        Tick {},
    }
}
