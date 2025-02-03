// Copyright 2025 Bloxide, all rights reserved

use super::{components::*, messaging::*, states::starting::Starting, states::*};
use bloxide::{core::components::*, runtime::*, std_exports::*};
use log::*;

#[cfg(feature = "runtime-tokio")]
impl Runnable<RootComponents> for Blox<RootComponents> {
    fn run(mut self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.state_machine
            .init(&RootStates::default(), &RootStates::Starting(Starting));
        Box::pin(async move {
            loop {
                select! {
                    Some(msg) = self.receivers.std_rx.recv() => {
                        trace!("Root Blox received a Standard message: {msg:?}");
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(RootMessage::StandardMessage(msg), &current_state);
                    }
                    Some(msg) = self.receivers.counter_rx.recv() => {
                        trace!("Root Blox received a Counter message: {msg:?}");
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(RootMessage::CounterMessage(msg), &current_state);
                    }
                    else => {
                        break;
                    }
                }
            }
        })
    }
}

use embassy_futures::select::{select, Either};

#[cfg(feature = "runtime-embassy")]
impl Runnable<RootComponents> for Blox<RootComponents> {
    fn run(mut self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.state_machine
            .init(&RootStates::default(), &RootStates::Starting(Starting));
        Box::pin(async move {
            loop {
                match select (
                    self.receivers.std_rx.channel.receiver().receive(),
                    self.receivers.counter_rx.channel.receiver().receive(),
                ).await {
                    Either::First(msg) => {
                        trace!("Root Blox received a Standard message: {msg:?}");
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(RootMessage::StandardMessage(msg), &current_state);
                    }
                    Either::Second(msg) => {
                        trace!("Root Blox received a Counter message: {msg:?}");
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(RootMessage::CounterMessage(msg), &current_state);
                    }
                   
                }
            }
        })
    }
}