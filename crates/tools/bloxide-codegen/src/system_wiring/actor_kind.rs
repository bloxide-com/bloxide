// Copyright 2025 Bloxide, all rights reserved
//! Actor-kind classification: which actors appear in the generated main body.

pub(super) fn crate_name(s: &str) -> String {
    s.replace("-", "_")
}

/// Returns true if this actor is a timer service actor (no blox.toml, no
/// channels, no task, no context — just a timer mailbox spawned directly).
pub(super) fn is_timer(actor: &crate::schema::ActorInstance) -> bool {
    actor.kind.as_deref() == Some("timer")
}

/// Returns true if this actor is dynamically spawned (concrete spec generated
/// at system level, but no channels/task/bootstrap/context in main.rs — the
/// impl crate's spawn function handles construction at runtime).
pub(super) fn is_dynamic(actor: &crate::schema::ActorInstance) -> bool {
    actor.kind.as_deref() == Some("dynamic")
}

/// Returns true if this actor should be skipped during main.rs body generation
/// (channels, tasks, context, machine, bootstrap). Both timer and dynamic
/// actors are skipped — they have no presence in the generated main function.
pub(super) fn skip_in_main_body(actor: &crate::schema::ActorInstance) -> bool {
    is_timer(actor) || is_dynamic(actor)
}
