// Copyright 2025 Bloxide, all rights reserved
//! Unified event type for the Worker actor.
//!
//! The `Mailboxes` tuple is:
//!   `(R::Stream<WorkerCtrl<R>>, R::Stream<WorkerMsg>)`
//!   index 0 = Ctrl (higher priority — ensures AddPeer runs before DoWork)
//!   index 1 = Msg  (domain messages)
//!
//! Uses `#[blox_event]` attribute macro (not `event!`) because `WorkerCtrl<R>`
//! does not implement `Debug`, preventing use of the `event!` macro which
//! always derives `Debug`.
use bloxide_core::capability::BloxRuntime;
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_core::messaging::Envelope;
use bloxide_macros::blox_event;
use pool_messages::{WorkerCtrl, WorkerMsg};

/// Combined event type for the Worker actor.
///
/// - Lifecycle: wraps Start/Reset/Stop/Ping commands
/// - Ctrl: Worker-control commands (AddPeer/RemovePeer), index 0, higher priority
/// - Msg: Domain messages (DoWork/PeerResult), index 1
///
/// Note: `Debug` is intentionally not derived — `WorkerCtrl<R>` does not
/// implement `Debug`, so the full enum cannot either.
#[blox_event]
pub enum WorkerEvent<R: BloxRuntime> {
    /// Lifecycle command (Start/Reset/Stop/Ping).
    Lifecycle(LifecycleCommand),
    /// Worker-control command (AddPeer / RemovePeer). Polled first (higher priority).
    Ctrl(Envelope<WorkerCtrl<R>>),
    /// Domain message (DoWork / PeerResult).
    Msg(Envelope<WorkerMsg>),
}
