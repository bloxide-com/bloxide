// Copyright 2025 Bloxide, all rights reserved

use super::{components::*, messaging::*, states::*};
use bloxide::{core::components::*, runtime::*, std_exports::*};
use log::*;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;


#[cfg(feature = "runtime-tokio")]
pub type CounterHandle = TokioHandle<CounterPayload>;

#[cfg(feature = "runtime-embassy")]
pub type CounterMutex = CriticalSectionRawMutex;
#[cfg(feature = "runtime-embassy")]
pub const COUNTER_CHANNEL_SIZE: usize = DEFAULT_CHANNEL_SIZE;
#[cfg(feature = "runtime-embassy")]
pub type CounterHandle = EmbassyHandle<CounterPayload, CounterMutex, COUNTER_CHANNEL_SIZE>;

pub type CounterBlox = Blox<CounterComponents>;

#[cfg(feature = "runtime-tokio")]
impl Runnable<CounterComponents> for Blox<CounterComponents> {
    fn run(mut self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.state_machine.init(
            &CounterStateEnum::Uninit(Uninit),
            &CounterStateEnum::NotStarted(NotStarted),
        );
        Box::pin(async move {
            loop {
                select! {
                    Some(message) = self.receivers.standard_receiver.recv() => {
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(CounterMessageSet::StandardMessage(message), &current_state);
                    },
                    Some(message) = self.receivers.counter_receiver.recv() => {
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(CounterMessageSet::CounterMessage(message), &current_state);
                    },
                    else => {
                        // If all channels closed, break out
                        trace!("All channels closed. Stopping run loop.");
                        break;
                    }
                }
            }
        })
    }
}

use embassy_futures::select::{select, Either};

impl Runnable<CounterComponents> for CounterBlox {
    fn run(mut self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        info!("Counter Blox starting");
        self.state_machine.init(
            &CounterStateEnum::Uninit(Uninit),
            &CounterStateEnum::NotStarted(NotStarted),
        );
        Box::pin(async move {
            loop {
                match select (
                    self.receivers.standard_receiver.channel.receiver().receive(),
                    self.receivers.counter_receiver.channel.receiver().receive(),
                ).await {
                    Either::First(message) => {
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(CounterMessageSet::StandardMessage(message), &current_state);
                    },
                    Either::Second(message) => {
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(CounterMessageSet::CounterMessage(message), &current_state);
                    },
                   
                }
            }
            info!("Counter Blox shutting down");
        })
    }
}
