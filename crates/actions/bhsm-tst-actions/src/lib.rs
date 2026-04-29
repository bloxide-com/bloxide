// Copyright 2025 Bloxide, all rights reserved
//! Action traits and generic functions for the bhsm-tst HSM topology demo.
#![no_std]

use bloxide_macros::delegatable;

pub mod prelude {
    pub use crate::HasPrintPrefix;
}

#[delegatable]
pub trait HasPrintPrefix {
    fn prefix(&self) -> &'static str;
    fn set_prefix(&mut self, prefix: &'static str);
}
