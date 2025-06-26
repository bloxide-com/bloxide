// Copyright 2025 Bloxide, all rights reserved

use bloxide_core::{
    blox::{
        demo_root::{components::*, ext_state::*},
        supervisor::{components::*, ext_state::*},
    },
    prelude::*,
};
use bloxide_tokio::{TokioMessageHandle, TokioRuntime, DEFAULT_CHANNEL_SIZE};
use log::*;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();
    // Create the Supervisor handle first so it can be passed to the Root
    let (supervisor_supervisor_handle, supervisor_supervisor_rx) =
        TokioMessageHandle::create_channel_with_size(11, DEFAULT_CHANNEL_SIZE);

    let (root_standard_handle, root_standard_rx) =
        TokioMessageHandle::create_channel_with_size(1, DEFAULT_CHANNEL_SIZE);

    let (root_counter_handle, root_counter_rx) =
        TokioMessageHandle::create_channel_with_size(2, DEFAULT_CHANNEL_SIZE);

    let root_receivers: <RootComponents<TokioRuntime> as Components>::Receivers = RootReceivers {
        std_rx: root_standard_rx,
        counter_rx: root_counter_rx,
    };

    let root_handles: <RootComponents<TokioRuntime> as Components>::Handles = RootHandles {
        standard_handle: root_standard_handle.clone(),
        counter_handle: root_counter_handle,
    };

    let root_init_args = RootInitArgs {
        supervisor_handle: supervisor_supervisor_handle.clone(),
        counter_handle: None,
    };

    let root_extended_state = RootExtState::<TokioRuntime>::new(root_init_args);

    // f) Construct the Root Blox
    let root_blox = Blox::<RootComponents<TokioRuntime>>::new(
        root_receivers,
        root_extended_state,
        root_handles,
    );

    let root_future = Box::pin(async move {
        Box::new(root_blox).run().await;
    });

    let (supervisor_standard_handle, supervisor_standard_rx) =
        TokioMessageHandle::create_channel_with_size(10, DEFAULT_CHANNEL_SIZE);

    let supervisor_receivers = SupervisorReceivers::<TokioRuntime> {
        standard_receiver: supervisor_standard_rx,
        supervisor_receiver: supervisor_supervisor_rx,
    };

    let supervisor_handles = SupervisorHandles::<TokioRuntime> {
        standard_handle: supervisor_standard_handle,
        supervisor_handle: supervisor_supervisor_handle,
    };

    let supervisor_init_args = SupervisorInitArgs::<TokioRuntime> {
        root_standard_handle,
        root_future,
    };

    let supervisor_extended_state =
        SupervisorExtendedState::<TokioRuntime>::new(supervisor_init_args);

    let supervisor_blox = Blox::<SupervisorComponents<TokioRuntime>>::new(
        supervisor_receivers,
        supervisor_extended_state,
        supervisor_handles,
    );

    tokio::spawn(async move {
        Box::new(supervisor_blox).run().await;
        info!("Supervisor finished.");
    });

    // Wait briefly to show them doing work
    sleep(Duration::from_secs(2)).await;
    info!("Main done!");
}
