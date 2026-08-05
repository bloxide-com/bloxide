// Copyright 2025 Bloxide, all rights reserved
//! Domain message types shared by the Ping and Pong bloxes.
#![no_std]

pub mod prelude;

pub mod generated;
pub use generated::{Ping, PingPongMsg, Pong, Resume};
