// Embassy static wiring demo (std target) — fully event-driven.
//
// HSM features demonstrated:
//   1. Deep hierarchy  — Root → Operating(Active, Paused) → Done
//   2. Event bubbling  — Paused bubbles stray Pong to Operating (Stay)
//   3. LCA exit order  — Active::on_exit AND Operating::on_exit fire on → Done
//   4. Timer-driven    — Paused::on_entry sets a timer; Resume arrives via mailbox
//   5. Supervision     — Generic supervisor manages child lifecycles
//   6. Clean shutdown  — Supervisor self-terminates via Guard::Reset (full LCA exit)
//   7. Tracing         — Engine emits trace! at every entry, exit, transition
//   8. Layered actions — Generic action crates with feature-gated logging
//   9. Timer service   — Dedicated timer actor using timer_task!/spawn_timer! macros
//
// Run with: RUST_LOG=trace cargo run --bin embassy-demo

extern crate alloc;

use bloxide_embassy::prelude::*;
use embassy_demo_impl::PingBehavior;
use ping_blox::prelude::*;
use ping_pong_messages::prelude::*;
use pong_blox::prelude::*;

// ── Embassy task wrappers ─────────────────────────────────────────────────────

bloxide_embassy::timer_task!(timer_task);
bloxide_embassy::root_task!(
    supervisor_task,
    SupervisorSpec<EmbassyRuntime>,
    std::process::exit(0)
);
bloxide_embassy::actor_task_supervised!(ping_task, PingSpec<EmbassyRuntime, PingBehavior>);
bloxide_embassy::actor_task_supervised!(pong_task, PongSpec<EmbassyRuntime>);

// ── Main ──────────────────────────────────────────────────────────────────────

static EXECUTOR: static_cell::StaticCell<embassy_executor::Executor> =
    static_cell::StaticCell::new();

fn main() {
    // Bridge `log`-crate messages (from bloxide-log's `log` feature) into
    // the tracing subscriber so they appear in the same log stream.
    tracing_log::LogTracer::init().unwrap();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("trace")),
        )
        .init();

    let executor = EXECUTOR.init(embassy_executor::Executor::new());
    executor.run(setup);
}

fn setup(spawner: Spawner) {
    let timer_ref = bloxide_embassy::spawn_timer!(spawner, timer_task, 8);

    let ((ping_ref,), ping_mbox) = bloxide_embassy::channels! {
        PingPongMsg(16),
    };
    let ping_id = ping_ref.id();

    let ((pong_ref,), pong_mbox) = bloxide_embassy::channels! {
        PingPongMsg(16),
    };
    let pong_id = pong_ref.id();

    tracing::info!(ping_id, pong_id, "setup");

    let ping_ctx = PingCtx::new(
        ping_id,
        pong_ref.clone(),
        ping_ref.clone(),
        timer_ref,
        PingBehavior::default(),
    );
    let pong_ctx = PongCtx::new(pong_id, ping_ref);

    let ping_machine = StateMachine::new(ping_ctx);
    let pong_machine = StateMachine::new(pong_ctx);

    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
    bloxide_embassy::spawn_child!(
        spawner,
        group,
        ping_task(ping_machine, ping_mbox, ping_id),
        ChildPolicy::Restart { max: 1 }
    );
    bloxide_embassy::spawn_child!(
        spawner,
        group,
        pong_task(pong_machine, pong_mbox, pong_id),
        ChildPolicy::Stop
    );
    let sup_id = bloxide_embassy::next_actor_id!();
    let (children, sup_notify_rx) = group.finish();

    tracing::info!(sup_id, "supervisor setup");

    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::new(sup_ctx);
    sup_machine.start();

    spawner.must_spawn(supervisor_task(sup_machine, (sup_notify_rx,)));
}
