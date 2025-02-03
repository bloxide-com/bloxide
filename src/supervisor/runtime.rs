// Copyright 2025 Bloxide, all rights reserved

use super::{components::*, messaging::*, states::*};
use crate::{core::components::*, runtime::*, std_exports::*};
use log::*;
use embassy_sync::once_lock::OnceLock;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_futures::select::{select, Either};

#[cfg(feature = "runtime-tokio")]
use std::sync::OnceLock;
#[cfg(feature = "runtime-tokio")]
pub type SupervisorHandle = TokioHandle<SupervisorPayload>;
#[cfg(feature = "runtime-tokio")]
pub type SupervisorLocalHandle = TokioHandle<SupervisorLocalPayload>;

#[cfg(feature = "runtime-embassy")]
pub type SupervisorMutex = CriticalSectionRawMutex;
#[cfg(feature = "runtime-embassy")]
pub const SUPERVISOR_CHANNEL_SIZE: usize = DEFAULT_CHANNEL_SIZE;
#[cfg(feature = "runtime-embassy")]
pub type SupervisorHandle = EmbassyHandle<SupervisorPayload, SupervisorMutex, SUPERVISOR_CHANNEL_SIZE>;
#[cfg(feature = "runtime-embassy")]
pub type SupervisorLocalHandle = EmbassyHandle<SupervisorLocalPayload, SupervisorMutex, SUPERVISOR_CHANNEL_SIZE>;
#[cfg(feature = "runtime-embassy")]
pub static SUPERVISOR_HANDLE: OnceLock<SupervisorHandle> = OnceLock::new();
#[cfg(feature = "runtime-embassy")]
pub fn init_supervisor_handle(handle: SupervisorHandle) {
    SUPERVISOR_HANDLE.get_or_init(|| handle);
}
#[cfg(feature = "runtime-embassy")]
pub fn get_supervisor_handle() -> &'static SupervisorHandle {
    match SUPERVISOR_HANDLE.try_get() {
        Some(handle) => handle,
        None => panic!("Supervisor handle not initialized!")
    }
}
#[cfg(feature = "runtime-embassy")]
pub static SUPERVISORLOCAL_HANDLE: OnceLock<SupervisorLocalHandle> = OnceLock::new();
#[cfg(feature = "runtime-embassy")]
pub fn init_local_supervisor_handle(handle: SupervisorLocalHandle) {
    SUPERVISORLOCAL_HANDLE.get_or_init(|| handle);
}
#[cfg(feature = "runtime-embassy")]
pub async fn get_local_supervisor_handle() -> &'static SupervisorLocalHandle {
    SUPERVISORLOCAL_HANDLE.get().await
}


#[cfg(feature = "runtime-tokio")]
impl Runnable<SupervisorComponents> for Blox<SupervisorComponents> {
    fn run(mut self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.state_machine.init(
            &SupervisorStateEnum::Uninit(Uninit),
            &SupervisorStateEnum::Running(Running),
        );
        Box::pin(async move {
            trace!("Supervisor started. Listening for messages...");
            loop {
                select! {
                    Some(message) = self.receivers.standard_receiver.recv() => {
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(SupervisorMessageSet::StandardMessage(message), &current_state);
                    },
                    Some(message) = self.receivers.supervisor_receiver.recv() => {
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(SupervisorMessageSet::SupervisorMessage(message), &current_state);
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

#[cfg(feature = "runtime-tokio")]
pub static SUPERVISOR_HANDLE: OnceLock<SupervisorHandle> = OnceLock::new();

#[cfg(feature = "runtime-tokio")]
thread_local! {
    pub static SUPERVISORLOCAL_HANDLE: OnceCell<SupervisorLocalHandle> = const {OnceCell::new()};
}

#[cfg(feature = "runtime-tokio")]
pub fn init_supervisor_handle(handle: SupervisorHandle) {
    SUPERVISOR_HANDLE
        .set(handle)
        .expect("Supervisor handle can only be initialized once!");
}

#[cfg(feature = "runtime-tokio")]
pub fn get_supervisor_handle() -> &'static SupervisorHandle {
    SUPERVISOR_HANDLE
        .get()
        .expect("Supervisor handle not initialized!")
}

#[cfg(feature = "runtime-tokio")]
pub fn init_local_supervisor_handle(handle: SupervisorLocalHandle) {
    SUPERVISORLOCAL_HANDLE.with(|cell| {
        cell.set(handle)
            .expect("Supervisor handle already initialized in this thread!");
    });
}

#[cfg(feature = "runtime-tokio")]
pub fn get_local_supervisor_handle() -> SupervisorLocalHandle {
    SUPERVISORLOCAL_HANDLE.with(|cell| {
        cell.get()
            .expect("Supervisor handle not initialized in this thread!")
            .clone()
    })
}


#[cfg(feature = "runtime-embassy")]
impl Runnable<SupervisorComponents> for Blox<SupervisorComponents> {
    fn run(mut self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.state_machine.init(
            &SupervisorStateEnum::Uninit(Uninit),
            &SupervisorStateEnum::Running(Running),
        );
        Box::pin(async move {
            trace!("Supervisor started. Listening for messages...");
            loop {
                match select(
                    self.receivers.standard_receiver.channel.receiver().receive(),
                    self.receivers.supervisor_receiver.channel.receiver().receive(),
                ).await {
                    Either::First(message) => {
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(SupervisorMessageSet::StandardMessage(message), &current_state);
                    }
                    Either::Second(message) => {
                        let current_state = self.state_machine.current_state.clone();
                        self.state_machine.dispatch(SupervisorMessageSet::SupervisorMessage(message), &current_state);
                    }
                }
            }
        })
    }
}