// ── Observability macros ──────────────────────────────────────────────────────
//
// Feature-gated tracing instrumentation for the HSM engine.
//
// Design: a single `_trace!` dispatch macro has two feature-gated definitions —
// one that emits live `tracing` calls and one that produces no-ops. All named
// macros below are thin wrappers that delegate to `_trace!`, so the
// `#[cfg(feature = "tracing")]` decision lives in exactly one place.

// ── Core dispatch ─────────────────────────────────────────────────────────────

#[cfg(feature = "tracing")]
macro_rules! _trace {
    (@on_entry $s:expr) => {
        tracing::trace!(state = ?$s, "on_entry");
    };
    (@on_exit $s:expr) => {
        tracing::trace!(state = ?$s, "on_exit");
    };
    (@init_entry) => {
        tracing::trace!("init_entry");
    };
    (@init_exit) => {
        tracing::trace!("init_exit");
    };
    (@init_drop $e:expr) => {
        let _ = &$e;
        tracing::trace!("init_drop_event");
    };
    (@on_event $s:expr, $e:expr) => {
        tracing::trace!(state = ?$s, "on_event_received");
    };
    (@on_transition $src:expr, $tgt:expr, $lca:expr) => {
        tracing::trace!(source = ?$src, target = ?$tgt, lca = ?$lca, "on_transition");
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! _trace {
    (@on_entry $s:expr) => {
        let _ = &$s;
    };
    (@on_exit $s:expr) => {
        let _ = &$s;
    };
    (@init_entry) => {};
    (@init_exit) => {};
    (@init_drop $e:expr) => {
        let _ = &$e;
    };
    (@on_event $s:expr, $e:expr) => {
        let _ = (&$s, &$e);
    };
    (@on_transition $src:expr, $tgt:expr, $lca:expr) => {
        let _ = (&$src, &$tgt, &$lca);
    };
}

// ── Named wrappers ────────────────────────────────────────────────────────────
// Each macro is a one-line delegation to `_trace!` with a descriptive tag.
// To add a new trace point: add an arm to both `_trace!` definitions above,
// then add a named wrapper here.

macro_rules! trace_on_entry {
    ($s:expr) => { _trace!(@on_entry $s) };
}

macro_rules! trace_on_exit {
    ($s:expr) => { _trace!(@on_exit $s) };
}

macro_rules! trace_init_entry {
    () => { _trace!(@init_entry) };
}

macro_rules! trace_init_exit {
    () => { _trace!(@init_exit) };
}

macro_rules! trace_init_drop_event {
    ($e:expr) => { _trace!(@init_drop $e) };
}

macro_rules! trace_on_event_received {
    ($s:expr, $e:expr) => { _trace!(@on_event $s, $e) };
}

macro_rules! trace_on_transition {
    ($src:expr, $tgt:expr, $lca:expr) => { _trace!(@on_transition $src, $tgt, $lca) };
}
