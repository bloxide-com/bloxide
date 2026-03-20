// Copyright 2025 Bloxide, all rights reserved
//! Unified event type for the Pool actor.
//!
//! The pool has a single domain mailbox: `R::Stream<PoolMsg>`.
use bloxide_macros::event;
use pool_messages::PoolMsg;

event!(Pool { Msg: PoolMsg });
