// Copyright 2025 Bloxide, all rights reserved
use crate::prelude::*;
use core::task::{Context, Poll};
use futures_core::Stream;

/// An enum to tag which sub‐stream (S1 or S2) produced the item.
pub enum MergedItem<I1, I2> {
    From1(I1),
    From2(I2),
}

/// A simple two‐stream merger that yields items from either S1 or S2.
///
/// When polled, it tries S1 first; if that stream yields `Poll::Ready(...)`
/// we return that item. Otherwise, we try S2. If *both* are pending,
/// we return `Poll::Pending`. If both have ended, we return `None`.
pub struct MergedStream2<S1, S2> {
    s1_ended: bool,
    s2_ended: bool,
    s1: S1,
    s2: S2,
}

impl<S1, S2> MergedStream2<S1, S2> {
    pub fn new(s1: S1, s2: S2) -> Self {
        Self {
            s1,
            s2,
            s1_ended: false,
            s2_ended: false,
        }
    }
}

impl<S1, S2, I1, I2> Stream for MergedStream2<S1, S2>
where
    S1: Stream<Item = I1> + Unpin,
    S2: Stream<Item = I2> + Unpin,
{
    type Item = MergedItem<I1, I2>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If s1 not ended, poll it
        if !self.s1_ended {
            match Pin::new(&mut self.s1).poll_next(cx) {
                Poll::Ready(Some(item)) => {
                    return Poll::Ready(Some(MergedItem::From1(item)));
                }
                Poll::Ready(None) => {
                    // s1 ended
                    self.s1_ended = true;
                }
                Poll::Pending => {
                    // We’ll keep going and check s2
                }
            }
        }

        if !self.s2_ended {
            match Pin::new(&mut self.s2).poll_next(cx) {
                Poll::Ready(Some(item)) => {
                    return Poll::Ready(Some(MergedItem::From2(item)));
                }
                Poll::Ready(None) => {
                    // s2 ended
                    self.s2_ended = true;
                }
                Poll::Pending => {
                    // If s1 was pending and s2 is pending,
                    // we have to return Pending
                }
            }
        }

        if self.s1_ended && self.s2_ended {
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}
