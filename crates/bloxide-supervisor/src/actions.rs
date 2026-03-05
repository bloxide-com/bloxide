// Copyright 2025 Bloxide, all rights reserved
use crate::registry::ChildGroup;
use bloxide_core::{accessor::HasSelfId, capability::BloxRuntime};

pub trait HasChildren<R: BloxRuntime> {
    fn children(&self) -> &ChildGroup<R>;
}

pub fn start_children<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasChildren<R>,
{
    ctx.children().start_all(ctx.self_id());
}

pub fn stop_all_children<R, C>(ctx: &mut C)
where
    R: BloxRuntime,
    C: HasSelfId + HasChildren<R>,
{
    ctx.children().stop_all(ctx.self_id());
}
