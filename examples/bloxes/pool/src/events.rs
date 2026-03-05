// Copyright 2025 Bloxide, all rights reserved
//! Unified event type for the Pool actor.
//!
//! The pool has a single domain mailbox: `R::Stream<PoolMsg>`.
use bloxide_core::messaging::Envelope;
use bloxide_macros::blox_event;
use pool_messages::PoolMsg;

/// Combined event type for the Pool actor.
///
/// `#[blox_event]` generates:
/// - `From<Envelope<PoolMsg>>` for `PoolEvent`
/// - `EventTag` impl: Msg → 0
/// - Payload accessor: `msg_payload()`
#[blox_event]
#[derive(Debug)]
pub enum PoolEvent {
    /// Domain message (SpawnWorker / WorkDone).
    Msg(Envelope<PoolMsg>),
}
