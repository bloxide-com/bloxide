//! Feature-gated logging macros for the bloxide framework.
//!
//! This crate provides `blox_log_trace!`, `blox_log_debug!`, `blox_log_info!`,
//! `blox_log_warn!`, and `blox_log_error!` macros that dispatch to either `defmt`
//! or `log` based on Cargo features. Enable `log` for standard logging, or `defmt`
//! for embedded (defmt wins if both are enabled). With neither feature, macros
//! expand to no-ops.
//!
//! Downstream crates enable logging by depending on `bloxide-log` with the
//! desired feature (e.g. `features = ["log"]`). The macros are safe to call
//! from any crate regardless of which feature is active.
//!
//! # Backend Compatibility
//!
//! The two backends have different format-string requirements:
//!
//! - **`defmt` feature**: the format string must be a **string literal**
//!   (`$fmt:literal`). Passing a variable or any non-literal expression as the
//!   format argument will fail to compile.
//! - **`log` feature**: the format string accepts arbitrary token trees
//!   (`$($arg:tt)*`), so variables and expressions are allowed.
//!
//! Code that passes a variable as the format string will compile under the
//! `log` backend but fail under `defmt`. Always use a string literal for the
//! format argument to ensure portability across both backends.

#![no_std]

// Re-export backends so macros can reference them via $crate paths.
// This means calling crates don't need log/defmt as direct dependencies.

#[cfg(feature = "defmt")]
#[doc(hidden)]
pub use defmt as __defmt;

#[cfg(all(feature = "log", not(feature = "defmt")))]
#[doc(hidden)]
pub use log as __log;

// ── defmt backend (highest priority) ────────────────────────────────────────

#[cfg(feature = "defmt")]
#[macro_export]
macro_rules! blox_log_trace {
    ($actor_id:expr, $fmt:literal $(, $arg:expr)* $(,)?) => {
        $crate::__defmt::trace!(concat!("[{}] ", $fmt), $actor_id $(, $arg)*)
    };
}

#[cfg(feature = "defmt")]
#[macro_export]
macro_rules! blox_log_info {
    ($actor_id:expr, $fmt:literal $(, $arg:expr)* $(,)?) => {
        $crate::__defmt::info!(concat!("[{}] ", $fmt), $actor_id $(, $arg)*)
    };
}

#[cfg(feature = "defmt")]
#[macro_export]
macro_rules! blox_log_debug {
    ($actor_id:expr, $fmt:literal $(, $arg:expr)* $(,)?) => {
        $crate::__defmt::debug!(concat!("[{}] ", $fmt), $actor_id $(, $arg)*)
    };
}

#[cfg(feature = "defmt")]
#[macro_export]
macro_rules! blox_log_warn {
    ($actor_id:expr, $fmt:literal $(, $arg:expr)* $(,)?) => {
        $crate::__defmt::warn!(concat!("[{}] ", $fmt), $actor_id $(, $arg)*)
    };
}

#[cfg(feature = "defmt")]
#[macro_export]
macro_rules! blox_log_error {
    ($actor_id:expr, $fmt:literal $(, $arg:expr)* $(,)?) => {
        $crate::__defmt::error!(concat!("[{}] ", $fmt), $actor_id $(, $arg)*)
    };
}

// ── log backend ─────────────────────────────────────────────────────────────

#[cfg(all(feature = "log", not(feature = "defmt")))]
#[macro_export]
macro_rules! blox_log_trace {
    ($actor_id:expr, $($arg:tt)*) => {
        $crate::__log::trace!("[{}] {}", $actor_id, core::format_args!($($arg)*))
    };
}

#[cfg(all(feature = "log", not(feature = "defmt")))]
#[macro_export]
macro_rules! blox_log_info {
    ($actor_id:expr, $($arg:tt)*) => {
        $crate::__log::info!("[{}] {}", $actor_id, core::format_args!($($arg)*))
    };
}

#[cfg(all(feature = "log", not(feature = "defmt")))]
#[macro_export]
macro_rules! blox_log_debug {
    ($actor_id:expr, $($arg:tt)*) => {
        $crate::__log::debug!("[{}] {}", $actor_id, core::format_args!($($arg)*))
    };
}

#[cfg(all(feature = "log", not(feature = "defmt")))]
#[macro_export]
macro_rules! blox_log_warn {
    ($actor_id:expr, $($arg:tt)*) => {
        $crate::__log::warn!("[{}] {}", $actor_id, core::format_args!($($arg)*))
    };
}

#[cfg(all(feature = "log", not(feature = "defmt")))]
#[macro_export]
macro_rules! blox_log_error {
    ($actor_id:expr, $($arg:tt)*) => {
        $crate::__log::error!("[{}] {}", $actor_id, core::format_args!($($arg)*))
    };
}

// ── no-op (neither feature enabled) ─────────────────────────────────────────

#[cfg(not(any(feature = "defmt", feature = "log")))]
#[macro_export]
macro_rules! blox_log_trace {
    ($actor_id:expr, $($arg:tt)*) => {
        if false { let _ = &$actor_id; core::format_args!($($arg)*); }
    };
}

#[cfg(not(any(feature = "defmt", feature = "log")))]
#[macro_export]
macro_rules! blox_log_info {
    ($actor_id:expr, $($arg:tt)*) => {
        if false { let _ = &$actor_id; core::format_args!($($arg)*); }
    };
}

#[cfg(not(any(feature = "defmt", feature = "log")))]
#[macro_export]
macro_rules! blox_log_debug {
    ($actor_id:expr, $($arg:tt)*) => {
        if false { let _ = &$actor_id; core::format_args!($($arg)*); }
    };
}

#[cfg(not(any(feature = "defmt", feature = "log")))]
#[macro_export]
macro_rules! blox_log_warn {
    ($actor_id:expr, $($arg:tt)*) => {
        if false { let _ = &$actor_id; core::format_args!($($arg)*); }
    };
}

#[cfg(not(any(feature = "defmt", feature = "log")))]
#[macro_export]
macro_rules! blox_log_error {
    ($actor_id:expr, $($arg:tt)*) => {
        if false { let _ = &$actor_id; core::format_args!($($arg)*); }
    };
}
