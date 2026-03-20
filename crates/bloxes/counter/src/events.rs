// Copyright 2025 Bloxide, all rights reserved
//! Unified event type for the counter blox.
use bloxide_macros::event;
use counter_messages::CounterMsg;

event!(Counter { Msg: CounterMsg });
