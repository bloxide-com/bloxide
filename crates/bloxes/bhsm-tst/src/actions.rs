// Copyright 2025 Bloxide, all rights reserved
//! Action functions for BhsmTst — entry/exit traces and event actions.
use bloxide_core::transition::ActionResult;

use crate::{BhsmTstCtx, BhsmTstEvent, BhsmTstSpec};

#[cfg(feature = "std")]
macro_rules! trace {
    ($($arg:tt)*) => { std::println!($($arg)*); };
}
#[cfg(not(feature = "std"))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

impl BhsmTstSpec {
    pub(crate) fn s_entry(_ctx: &mut BhsmTstCtx) {
        trace!("s-ENTRY;");
    }
    pub(crate) fn s_exit(_ctx: &mut BhsmTstCtx) {
        trace!("s-EXIT;");
    }
    pub(crate) fn s1_entry(_ctx: &mut BhsmTstCtx) {
        trace!("s1-ENTRY;");
    }
    pub(crate) fn s1_exit(_ctx: &mut BhsmTstCtx) {
        trace!("s1-EXIT;");
    }
    pub(crate) fn s11_entry(_ctx: &mut BhsmTstCtx) {
        trace!("s11-ENTRY;");
    }
    pub(crate) fn s11_exit(_ctx: &mut BhsmTstCtx) {
        trace!("s11-EXIT;");
    }
    pub(crate) fn s2_entry(_ctx: &mut BhsmTstCtx) {
        trace!("s2-ENTRY;");
    }
    pub(crate) fn s2_exit(_ctx: &mut BhsmTstCtx) {
        trace!("s2-EXIT;");
    }
    pub(crate) fn s21_entry(_ctx: &mut BhsmTstCtx) {
        trace!("s21-ENTRY;");
    }
    pub(crate) fn s21_exit(_ctx: &mut BhsmTstCtx) {
        trace!("s21-EXIT;");
    }
    pub(crate) fn s211_entry(_ctx: &mut BhsmTstCtx) {
        trace!("s211-ENTRY;");
    }
    pub(crate) fn s211_exit(_ctx: &mut BhsmTstCtx) {
        trace!("s211-EXIT;");
    }
    pub(crate) fn error_entry(_ctx: &mut BhsmTstCtx) {
        trace!("error-ENTRY;");
    }
    pub(crate) fn error_exit(_ctx: &mut BhsmTstCtx) {
        trace!("error-EXIT;");
    }
    pub(crate) fn done_entry(_ctx: &mut BhsmTstCtx) {
        trace!("done-ENTRY;");
    }
    pub(crate) fn done_exit(_ctx: &mut BhsmTstCtx) {
        trace!("done-EXIT;");
    }

    pub(crate) fn s_i(_ctx: &mut BhsmTstCtx, _ev: &BhsmTstEvent) -> ActionResult {
        trace!("s-I;");
        ActionResult::Ok
    }

    pub(crate) fn s11_a(_ctx: &mut BhsmTstCtx, _ev: &BhsmTstEvent) -> ActionResult {
        trace!("s11-A;");
        ActionResult::Ok
    }

    pub(crate) fn s11_b(_ctx: &mut BhsmTstCtx, _ev: &BhsmTstEvent) -> ActionResult {
        trace!("s11-B;");
        ActionResult::Ok
    }
}
