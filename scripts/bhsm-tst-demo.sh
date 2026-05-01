#!/bin/bash
set -e

# Build the cargo-blox tool first
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"
cargo build -p cargo-blox --quiet

BLOX="cargo run -p cargo-blox --quiet -- blox"

DEMO="demo/bhsm-tst"
rm -rf "$REPO_ROOT/$DEMO"
mkdir -p "$REPO_ROOT/$DEMO"
cd "$REPO_ROOT/$DEMO"

# ── Workspace Cargo.toml ────────────────────────────────────────────────────
cat > Cargo.toml <<'WORKSPACE'
[workspace]
members = [
    "crates/messages/bhsm-tst-messages",
    "crates/actions/bhsm-tst-actions",
    "crates/bloxes/bhsm-tst",
    "apps/bhsm-tst-demo",
]
resolver = "2"

[workspace.package]
version = "0.0.3"
edition = "2021"

[workspace.dependencies]
bloxide-core          = { path = "../../../crates/bloxide-core" }
bloxide-tokio         = { path = "../../../runtimes/bloxide-tokio" }
bloxide-macros        = { path = "../../../crates/bloxide-macros" }
bloxide-log           = { path = "../../../crates/bloxide-log", features = ["log"] }
bhsm-tst-messages     = { path = "crates/messages/bhsm-tst-messages" }
bhsm-tst-actions      = { path = "crates/actions/bhsm-tst-actions" }
bhsm-tst-blox         = { path = "crates/bloxes/bhsm-tst" }

[profile.dev]
panic = "abort"
WORKSPACE

# ── Layer 1: Messages (11 unit variants: A, B, C, D, E, F, G, H, I, K, X) ─
$BLOX new-messages bhsm-tst
for ev in A B C D E F G H I K X; do
    $BLOX add-message bhsm-tst-messages "$ev"
done

# ── Layer 2: Actions (minimal — no mutable state needed) ─────────────────────
$BLOX new-actions bhsm-tst

# ── Layer 4: Blox (deep 3-level hierarchy) ─────────────────────────────────
$BLOX new bhsm-tst --messages bhsm-tst-messages --actions bhsm-tst-actions
$BLOX add-state bhsm-tst S --composite
$BLOX add-state bhsm-tst S1 --parent S --composite
$BLOX add-state bhsm-tst S11 --parent S1
$BLOX add-state bhsm-tst S2 --parent S --composite
$BLOX add-state bhsm-tst S21 --parent S2 --composite
$BLOX add-state bhsm-tst S211 --parent S21
$BLOX add-state bhsm-tst Error
$BLOX add-state bhsm-tst Done

# Generate boilerplate from TOML
$BLOX generate

# ── Write action functions (the only user-edited file) ──────────────────────
cat > crates/bloxes/bhsm-tst/src/actions.rs <<'ACTIONS'
use core::marker::PhantomData;

use bloxide_core::{
    capability::BloxRuntime,
    spec::StateFns,
    transition::ActionResult,
    transitions,
};
use bhsm_tst_messages::BhsmTstMsg;

use crate::bhsm_tst_state_handler_table;
use crate::{BhsmTstCtx, BhsmTstEvent};

pub use crate::generated::topology::BhsmTstState;

#[cfg(feature = "std")]
macro_rules! trace {
    ($($arg:tt)*) => { std::println!($($arg)*); };
}
#[cfg(not(feature = "std"))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

pub struct BhsmTstSpec<R: BloxRuntime>(PhantomData<R>);

impl<R: BloxRuntime> BhsmTstSpec<R> {
    fn s_entry(_ctx: &mut BhsmTstCtx) { trace!("s-ENTRY;"); }
    fn s_exit(_ctx: &mut BhsmTstCtx) { trace!("s-EXIT;"); }
    fn s1_entry(_ctx: &mut BhsmTstCtx) { trace!("s1-ENTRY;"); }
    fn s1_exit(_ctx: &mut BhsmTstCtx) { trace!("s1-EXIT;"); }
    fn s11_entry(_ctx: &mut BhsmTstCtx) { trace!("s11-ENTRY;"); }
    fn s11_exit(_ctx: &mut BhsmTstCtx) { trace!("s11-EXIT;"); }
    fn s2_entry(_ctx: &mut BhsmTstCtx) { trace!("s2-ENTRY;"); }
    fn s2_exit(_ctx: &mut BhsmTstCtx) { trace!("s2-EXIT;"); }
    fn s21_entry(_ctx: &mut BhsmTstCtx) { trace!("s21-ENTRY;"); }
    fn s21_exit(_ctx: &mut BhsmTstCtx) { trace!("s21-EXIT;"); }
    fn s211_entry(_ctx: &mut BhsmTstCtx) { trace!("s211-ENTRY;"); }
    fn s211_exit(_ctx: &mut BhsmTstCtx) { trace!("s211-EXIT;"); }
    fn error_entry(_ctx: &mut BhsmTstCtx) { trace!("error-ENTRY;"); }
    fn error_exit(_ctx: &mut BhsmTstCtx) { trace!("error-EXIT;"); }
    fn done_entry(_ctx: &mut BhsmTstCtx) { trace!("done-ENTRY;"); }
    fn done_exit(_ctx: &mut BhsmTstCtx) { trace!("done-EXIT;"); }

    fn s_i(_ctx: &mut BhsmTstCtx, _ev: &BhsmTstEvent) -> ActionResult {
        trace!("s-I;");
        ActionResult::Ok
    }

    fn s11_a(_ctx: &mut BhsmTstCtx, _ev: &BhsmTstEvent) -> ActionResult {
        trace!("s11-A;");
        ActionResult::Ok
    }

    fn s11_b(_ctx: &mut BhsmTstCtx, _ev: &BhsmTstEvent) -> ActionResult {
        trace!("s11-B;");
        ActionResult::Ok
    }

    const S_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s_entry],
        on_exit: &[Self::s_exit],
        transitions: transitions![
            BhsmTstMsg::H(_) => {
                transition BhsmTstState::S11
            },
            BhsmTstMsg::I(_) => {
                actions [Self::s_i]
                guard(_ctx, _results) {
                    _ => stay,
                }
            },
            BhsmTstMsg::K(_) => {
                transition BhsmTstState::Error
            },
            BhsmTstMsg::X(_) => {
                transition BhsmTstState::Done
            },
        ],
    };

    const S1_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s1_entry],
        on_exit: &[Self::s1_exit],
        transitions: transitions![
            BhsmTstMsg::C(_) => {
                transition BhsmTstState::S211
            },
        ],
    };

    const S11_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s11_entry],
        on_exit: &[Self::s11_exit],
        transitions: transitions![
            BhsmTstMsg::A(_) => {
                actions [Self::s11_a]
                transition BhsmTstState::S11
            },
            BhsmTstMsg::B(_) => {
                actions [Self::s11_b]
                transition BhsmTstState::S11
            },
            BhsmTstMsg::D(_) => {
                transition BhsmTstState::S211
            },
        ],
    };

    const S2_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s2_entry],
        on_exit: &[Self::s2_exit],
        transitions: &[],
    };

    const S21_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s21_entry],
        on_exit: &[Self::s21_exit],
        transitions: transitions![
            BhsmTstMsg::E(_) => {
                transition BhsmTstState::S211
            },
            BhsmTstMsg::G(_) => {
                transition BhsmTstState::S11
            },
        ],
    };

    const S211_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::s211_entry],
        on_exit: &[Self::s211_exit],
        transitions: transitions![
            BhsmTstMsg::F(_) => {
                transition BhsmTstState::S11
            },
        ],
    };

    const ERROR_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::error_entry],
        on_exit: &[Self::error_exit],
        transitions: &[],
    };

    const DONE_FNS: StateFns<Self> = StateFns {
        on_entry: &[Self::done_entry],
        on_exit: &[Self::done_exit],
        transitions: &[],
    };
}

impl<R: BloxRuntime> bloxide_core::spec::MachineSpec for BhsmTstSpec<R> {
    type State = BhsmTstState;
    type Event = BhsmTstEvent;
    type Ctx = BhsmTstCtx;
    type Mailboxes<Rt: BloxRuntime> = (Rt::Stream<BhsmTstMsg>,);

    const HANDLER_TABLE: &'static [&'static StateFns<Self>] = bhsm_tst_state_handler_table!(Self);

    fn initial_state() -> BhsmTstState {
        BhsmTstState::S11
    }

    fn is_terminal(state: &BhsmTstState) -> bool {
        matches!(state, BhsmTstState::Done)
    }

    fn is_error(state: &BhsmTstState) -> bool {
        matches!(state, BhsmTstState::Error)
    }

    fn on_init_entry(_ctx: &mut BhsmTstCtx) {}
}
ACTIONS

# ── Overwrite ctx.rs — no behavior field for pure topology demo ─────────────
cat > crates/bloxes/bhsm-tst/src/ctx.rs <<'CTX'
use bloxide_core::ActorId;
use bloxide_macros::BloxCtx;

#[derive(BloxCtx)]
pub struct BhsmTstCtx {
    pub self_id: ActorId,
}
CTX

# ── Overwrite lib.rs — export spec from actions module ──────────────────────
cat > crates/bloxes/bhsm-tst/src/lib.rs <<'LIB'
#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod prelude;

#[macro_use]
pub mod generated;

mod ctx;
mod events;
mod actions;

pub use ctx::BhsmTstCtx;
pub use events::BhsmTstEvent;
pub use actions::{BhsmTstSpec, BhsmTstState};
LIB

# ── Overwrite prelude.rs ────────────────────────────────────────────────────
cat > crates/bloxes/bhsm-tst/src/prelude.rs <<'PRELUDE'
pub use crate::{BhsmTstCtx, BhsmTstEvent, BhsmTstSpec, BhsmTstState};
PRELUDE

# ── Overwrite actions crate with minimal content ───────────────────────────
cat > crates/actions/bhsm-tst-actions/src/lib.rs <<'ACTIONS_LIB'
#![no_std]

use bloxide_macros::delegatable;

pub mod prelude {
    pub use crate::*;
}

#[delegatable]
pub trait HasPrintPrefix {
    fn prefix(&self) -> &'static str;
    fn set_prefix(&mut self, prefix: &'static str);
}
ACTIONS_LIB

# ── Layer 5: Binary ─────────────────────────────────────────────────────────
mkdir -p apps/bhsm-tst-demo/src

cat > apps/bhsm-tst-demo/Cargo.toml <<'CRATE'
[package]
name = "bhsm-tst-demo"
version.workspace = true
edition.workspace = true
publish = false

[dependencies]
bloxide-core      = { workspace = true, features = ["std"] }
bloxide-tokio     = { workspace = true }
bloxide-log       = { workspace = true }
bhsm-tst-blox     = { workspace = true }
bhsm-tst-messages = { workspace = true }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-log = "0.2"
CRATE

cat > apps/bhsm-tst-demo/src/main.rs <<'MAIN'
use bloxide_core::lifecycle::LifecycleCommand;
use bloxide_tokio::prelude::*;
use bhsm_tst_blox::prelude::*;
use bhsm_tst_messages::prelude::*;
use std::time::Duration;
use tokio::io::{self, AsyncBufReadExt};

bloxide_tokio::actor_task_supervised!(bhsm_task, BhsmTstSpec<TokioRuntime>);
bloxide_tokio::root_task!(supervisor_task, SupervisorSpec<TokioRuntime>);

fn print_usage() {
    eprintln!("BhsmTst commands:");
    eprintln!("  A B C D E F G H I K X  — send BhsmTstMsg variant to actor");
    eprintln!("  K  — trigger error (supervisor restarts actor)");
    eprintln!("  X  — terminal Done (supervisor shuts down)");
    eprintln!("  ?  — print this help");
}

#[tokio::main]
async fn main() {
    tracing_log::LogTracer::init().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init()
        .ok();

    let ((bhsm_ref,), bhsm_mbox) = bloxide_tokio::channels! {
        BhsmTstMsg(16),
    };
    let bhsm_id = bhsm_ref.id();

    tracing::info!(bhsm_id, "setting up BhsmTst actor");

    let bhsm_ctx = BhsmTstCtx::new(bloxide_tokio::next_actor_id!());
    let bhsm_machine = StateMachine::new(bhsm_ctx);

    let mut group = ChildGroupBuilder::new(GroupShutdown::WhenAnyDone);
    bloxide_tokio::spawn_child!(
        group,
        bhsm_task(bhsm_machine, bhsm_mbox, bhsm_id),
        ChildPolicy::Restart { max: 3 }
    );
    let sup_control_ref = group.control_ref();
    let _sup_notify = group.notify_sender();
    let sup_id = bloxide_tokio::next_actor_id!();
    let (children, sup_notify_rx, sup_control_rx) = group.finish();

    tracing::info!(sup_id, "supervisor setup");

    let sup_ctx = SupervisorCtx::new(sup_id, children);
    let mut sup_machine = StateMachine::<SupervisorSpec<TokioRuntime>>::new(sup_ctx);
    sup_machine.dispatch(SupervisorEvent::<TokioRuntime>::Lifecycle(
        LifecycleCommand::Start,
    ));

    let health_ref = sup_control_ref.clone();
    let _health_task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        loop {
            ticker.tick().await;
            if health_ref
                .try_send(sup_id, SupervisorControl::HealthCheckTick)
                .is_err()
            {
                break;
            }
        }
    });

    let stdin_ref = bhsm_ref.clone();
    let _stdin_task = tokio::spawn(async move {
        let stdin = io::BufReader::new(io::stdin());
        let mut lines = stdin.lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let cmd = line.trim().to_uppercase();
                    let ch = cmd.chars().next();
                    match ch {
                        Some('A') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::A(A)); }
                        Some('B') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::B(B)); }
                        Some('C') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::C(C)); }
                        Some('D') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::D(D)); }
                        Some('E') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::E(E)); }
                        Some('F') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::F(F)); }
                        Some('G') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::G(G)); }
                        Some('H') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::H(H)); }
                        Some('I') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::I(I)); }
                        Some('K') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::K(K)); }
                        Some('X') => { let _ = stdin_ref.try_send(bhsm_id, BhsmTstMsg::X(X)); }
                        Some('?') => print_usage(),
                        _ => eprintln!("unknown command: {:?}", cmd),
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
    });

    print_usage();
    supervisor_task(sup_machine, (sup_notify_rx, sup_control_rx)).await;
}
MAIN

echo "=== Setup complete. Running demo... ==="
cargo run -p bhsm-tst-demo
