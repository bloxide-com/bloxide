// Copyright 2025 Bloxide, all rights reserved

mod actors;

use crate::actors::counter::*;
use bloxide::core::actor::*;
use bloxide::core::messaging::*;
use bloxide::runtime::*;
use log::*;

#[::tokio::main]
async fn main() {
    env_logger::builder().init();

    // Make a handles
    //TODO: Integrate these steps into one new() function
    let (standard_sender, standard_receiver) =
        mpsc::channel::<Message<StandardPayload>>(DEFAULT_CHANNEL_SIZE);
    let (counter_sender, counter_receiver) =
        mpsc::channel::<Message<CounterPayload>>(DEFAULT_CHANNEL_SIZE);
    let handle = StandardMessageHandle::new(1, standard_sender);
    let counter_handle = CounterHandle::new(2, counter_sender);

    // Create actor config
    let config = CounterActorConfig {
        standard_receiver,
        counter_receiver,
        counter_handle: counter_handle.clone(),
    };

    // Create the actor
    let actor =
        Actor::<CounterComponents>::new(handle.clone(), CounterExtendedState::new(), config);

    // Spawn the actor's run loop
    if let StandardPayload::SpawnRequest(spawn_fn) = Box::new(actor).into_request() {
        spawn(spawn_fn());
        let _ = handle.try_send(Message::new(999, StandardPayload::Initialize));
        sleep(Duration::from_millis(50)).await; //TODO: Bug - this delay needed to make sure the actor handles Init before other messages
    }

    // Send some counter messages:

    let _ = counter_handle.try_send(Message::new(0, CounterPayload::SetMax(Box::new(5))));

    let _ = counter_handle.try_send(Message::new(0, CounterPayload::SetMin(Box::new(0))));

    let _ = counter_handle.try_send(Message::new(
        0,
        CounterPayload::CountEvent(Box::new(CountEvent::StartCounting)),
    ));
    let _ = counter_handle.try_send(Message::new(0, CounterPayload::Increment(Box::new(3))));

    let _ = counter_handle.try_send(Message::new(
        0,
        CounterPayload::CountEvent(Box::new(CountEvent::GetCount)),
    ));

    let _ = counter_handle.try_send(Message::new(0, CounterPayload::Decrement(Box::new(1))));

    let _ = counter_handle.try_send(Message::new(
        0,
        CounterPayload::CountEvent(Box::new(CountEvent::GetCount)),
    ));

    let _ = counter_handle.try_send(Message::new(0, CounterPayload::Increment(Box::new(4))));

    //TODO: Use new ctrl-c macro
    sleep(Duration::from_secs(2)).await;
    info!("Done!");
}
