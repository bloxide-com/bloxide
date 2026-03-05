use crate::command::TimerCommand;

/// Trait that runtimes implement to provide a timer service.
///
/// The implementation bridges [`TimerQueue`](crate::TimerQueue) (shared data
/// structure) to the runtime's native timer primitive (e.g.
/// `embassy_time::Timer`, `tokio::time::sleep`).
///
/// Blox crates never use this trait as a bound. It is used by wiring macros
/// (e.g. `timer_task!`) and enforces that every runtime provides a compatible
/// timer service implementation.
///
/// [`TimerQueue`]: crate::TimerQueue
#[allow(async_fn_in_trait)]
pub trait TimerService: bloxide_core::capability::BloxRuntime {
    /// Run the timer service loop forever.
    ///
    /// Receives `TimerCommand` messages from the stream, manages pending
    /// timers via `TimerQueue`, and fires callbacks when deadlines expire
    /// using the runtime's native timer.
    async fn run_timer_service(stream: Self::Stream<TimerCommand>);
}
