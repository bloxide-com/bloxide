// Copyright 2025 Bloxide, all rights reserved
// wait for ctrl-c to exit an application
#[macro_export]
macro_rules! wait_for_ctrl_c {
    ($($actor:expr),+ $(,)?) => {{
        println!("Press Ctrl+C to exit");
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                println!("\nShutting down...");
                // Send shutdown messages
                $($actor.send(StandardMessage::Shutdown);)*
            }
            Err(err) => eprintln!("Unable to listen for shutdown signal: {}", err),
        }
    }};
}
