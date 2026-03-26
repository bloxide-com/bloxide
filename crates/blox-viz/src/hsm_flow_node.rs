// Copyright 2025 Bloxide, all rights reserved
//! One canvas widget: a **blox** drawn as a nested HSM using [`FlowPaintCtx`] (Bloxflow pixel path).

use bloxflow_core::{BloxFlowNode, FlowPaintCtx, Position, RectNode};

use crate::layout::{snapshot_to_flow, state_depths};
use crate::snapshot::{BloxDiagramSnapshot, example_ping_blox_snapshot};

const IMPLICIT_INIT: &str = "__blox_implicit_init";

/// Renders a [`BloxDiagramSnapshot`] inside a single graph node (UML-style nested states).
#[derive(Clone, Debug, PartialEq)]
pub struct HsmFlowNode {
    pub id: String,
    pub position: Position,
    pub width: f32,
    pub height: f32,
    pub selected: bool,
    pub draggable: bool,
    pub class_name: String,
    pub snapshot: BloxDiagramSnapshot,
}

impl HsmFlowNode {
    pub fn new(
        id: impl Into<String>,
        snapshot: BloxDiagramSnapshot,
        position: Position,
        width: f32,
        height: f32,
    ) -> Self {
        Self {
            id: id.into(),
            position,
            width: width.max(120.0),
            height: height.max(100.0),
            selected: false,
            draggable: true,
            class_name: String::new(),
            snapshot,
        }
    }

    pub fn with_class(mut self, class_name: impl Into<String>) -> Self {
        self.class_name = class_name.into();
        self
    }

    pub fn with_draggable(mut self, draggable: bool) -> Self {
        self.draggable = draggable;
        self
    }

    /// Example Ping blox topology (same data as SVG/PNG export).
    pub fn ping_example(position: Position, width: f32, height: f32) -> Self {
        Self::new("ping_blox", example_ping_blox_snapshot(), position, width, height)
    }

    fn paint_content(&self, ctx: &mut dyn FlowPaintCtx) {
        let px = self.position.x;
        let py = self.position.y;
        let bw = self.width;
        let bh = self.height;

        let pad = 10.0f32;
        let title_band = 24.0f32;

        // Title strip
        ctx.fill_rounded_rect(px, py, bw, title_band, 6.0, [0.12, 0.2, 0.34, 1.0]);
        ctx.draw_text_centered_in_rect(
            px,
            py,
            bw,
            title_band,
            &self.snapshot.blox_name,
            [0.88, 0.93, 1.0, 1.0],
            11.0,
        );

        let inner_x = px + pad;
        let inner_y = py + title_band + 4.0;
        let inner_w = (bw - 2.0 * pad).max(40.0);
        let inner_h = (bh - title_band - pad - 8.0).max(40.0);

        ctx.fill_rounded_rect(
            inner_x,
            inner_y,
            inner_w,
            inner_h,
            8.0,
            [0.06, 0.1, 0.16, 0.95],
        );
        ctx.stroke_rounded_rect(
            inner_x,
            inner_y,
            inner_w,
            inner_h,
            8.0,
            [0.35, 0.5, 0.68, 0.7],
            1.0,
        );

        let (mut inner, edges) = snapshot_to_flow(&self.snapshot);
        if inner.is_empty() {
            return;
        }

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for n in &inner {
            let p = n.pos();
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x + n.width());
            max_y = max_y.max(p.y + n.height());
        }
        if !min_x.is_finite() {
            return;
        }
        let gw = (max_x - min_x).max(1.0);
        let gh = (max_y - min_y).max(1.0);
        let scale = (inner_w / gw).min(inner_h / gh);
        let ox = inner_x + (inner_w - gw * scale) * 0.5;
        let oy = inner_y + (inner_h - gh * scale) * 0.5;

        let map_x = |lx: f32| ox + (lx - min_x) * scale;
        let map_y = |ly: f32| oy + (ly - min_y) * scale;
        let map_w = |lw: f32| lw * scale;
        let map_h = |lh: f32| lh * scale;

        // Horizontal bands by graph depth (leaves/init only) — makes row layout visible after
        // uniform fit-to-inner scaling, which otherwise hides depth-based placement.
        let mut depths = state_depths(&self.snapshot);
        if let Some(t) = self.snapshot.implicit_entry_target.as_ref() {
            let td = depths.get(t.as_str()).copied().unwrap_or(0);
            depths.insert(IMPLICIT_INIT.into(), td);
        }
        let max_d = inner
            .iter()
            .filter_map(|n| depths.get(n.id()).copied())
            .max()
            .unwrap_or(0);
        for d in 0..=max_d {
            let mut ymin = f32::INFINITY;
            let mut ymax = f32::NEG_INFINITY;
            let mut any = false;
            for n in &inner {
                let cn = n.class_name();
                let is_band_node = cn.contains("bv-leaf") || cn.contains("bv-init");
                if !is_band_node {
                    continue;
                }
                if n.id() == IMPLICIT_INIT {
                    continue;
                }
                let Some(&dd) = depths.get(n.id()) else {
                    continue;
                };
                if dd != d {
                    continue;
                }
                any = true;
                let p = n.pos();
                ymin = ymin.min(p.y);
                ymax = ymax.max(p.y + n.height());
            }
            if !any || !ymin.is_finite() {
                continue;
            }
            let y0 = map_y(ymin);
            let y1 = map_y(ymax);
            let h = (y1 - y0).max(1.0);
            let stripe = if (d % 2) == 0 {
                [0.18, 0.32, 0.52, 0.11]
            } else {
                [0.22, 0.18, 0.42, 0.11]
            };
            ctx.fill_rounded_rect(inner_x, y0, inner_w, h, 2.0, stripe);
        }

        let center_of = |n: &bloxflow_core::Node| {
            let p = n.pos();
            (
                map_x(p.x + n.width() * 0.5),
                map_y(p.y + n.height() * 0.5),
            )
        };

        // Transitions (skip hierarchy connectors — containment is visual).
        for e in &edges {
            if e.class_name.contains("bv-edge-hierarchy") {
                continue;
            }
            let Some(s) = inner.iter().find(|n| n.id() == e.source.as_str()) else {
                continue;
            };
            let Some(t) = inner.iter().find(|n| n.id() == e.target.as_str()) else {
                continue;
            };
            let (x0, y0) = center_of(s);
            let (x1, y1) = center_of(t);
            let rgba = edge_stroke_rgba(&e.class_name);
            let sw = (1.4f32).max(0.8 * scale);
            ctx.stroke_line(x0, y0, x1, y1, rgba, sw);
        }

        // States: large regions under smaller leaves
        inner.sort_by(|a, b| {
            let aa = a.width() * a.height();
            let bb = b.width() * b.height();
            aa.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
        });

        for n in &inner {
            let p = n.pos();
            let cn = n.class_name();
            if n.id() == IMPLICIT_INIT || cn.contains("bv-init") {
                let cx = map_x(p.x + n.width() * 0.5);
                let cy = map_y(p.y + n.height() * 0.5);
                let d = (n.width() * scale)
                    .min(n.height() * scale)
                    .clamp(5.5, 20.0);
                let x = cx - d * 0.5;
                let y = cy - d * 0.5;
                let r = d * 0.5;
                // UML initial pseudostate: filled disc + thin highlight ring (not a named state).
                ctx.fill_rounded_rect(x, y, d, d, r, [0.04, 0.06, 0.09, 1.0]);
                ctx.stroke_rounded_rect(
                    x,
                    y,
                    d,
                    d,
                    r,
                    [0.88, 0.92, 0.98, 0.85],
                    (0.9 * scale).max(0.6),
                );
                continue;
            }
            let x = map_x(p.x);
            let y = map_y(p.y);
            let w = map_w(n.width());
            let h = map_h(n.height());
            let (fill, stroke, r, fs) = substate_style(cn);
            ctx.fill_rounded_rect(x, y, w, h, r * scale.min(1.0), fill);
            ctx.stroke_rounded_rect(x, y, w, h, r * scale.min(1.0), stroke, 1.2);
            let font = (fs * scale).clamp(7.0, 14.0);
            ctx.draw_text_centered_in_rect(x, y, w, h, n.label(), [0.9, 0.94, 1.0, 1.0], font);
        }
    }
}

fn edge_stroke_rgba(class: &str) -> [f32; 4] {
    if class.contains("bv-edge-start") {
        [0.25, 0.88, 0.48, 1.0]
    } else if class.contains("bv-edge-lifecycle") || class.contains("bv-edge-root") {
        [0.74, 0.62, 0.98, 1.0]
    } else {
        [0.42, 0.76, 0.98, 1.0]
    }
}

fn substate_style(class: &str) -> ([f32; 4], [f32; 4], f32, f32) {
    if class.contains("bv-composite") {
        return (
            [0.1, 0.16, 0.24, 1.0],
            [0.55, 0.7, 0.88, 0.9],
            8.0,
            10.0,
        );
    }
    if class.contains("bv-leaf-root") {
        return (
            [0.14, 0.12, 0.1, 1.0],
            [0.85, 0.68, 0.38, 1.0],
            8.0,
            10.0,
        );
    }
    (
        [0.09, 0.14, 0.22, 1.0],
        [0.32, 0.48, 0.65, 1.0],
        8.0,
        11.0,
    )
}

impl BloxFlowNode for HsmFlowNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn pos(&self) -> Position {
        self.position
    }

    fn set_pos(&mut self, p: Position) {
        self.position = p;
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn selected(&self) -> bool {
        self.selected
    }

    fn set_selected(&mut self, value: bool) {
        self.selected = value;
    }

    fn draggable(&self) -> bool {
        self.draggable
    }

    fn class_name(&self) -> &str {
        &self.class_name
    }

    fn label(&self) -> &str {
        self.snapshot.blox_name.as_str()
    }

    fn paint(&self, ctx: &mut dyn FlowPaintCtx) {
        let px = self.position.x;
        let py = self.position.y;
        let bw = self.width;
        let bh = self.height;
        let outer_r = 12.0f32;
        let chrome_fill = if self.selected {
            [0.08, 0.12, 0.2, 1.0]
        } else {
            [0.05, 0.09, 0.14, 1.0]
        };
        let chrome_stroke = if self.selected {
            [0.55, 0.78, 1.0, 1.0]
        } else {
            [0.28, 0.4, 0.55, 0.85]
        };
        ctx.fill_rounded_rect(px, py, bw, bh, outer_r, chrome_fill);
        ctx.stroke_rounded_rect(px, py, bw, bh, outer_r, chrome_stroke, 2.0);
        self.paint_content(ctx);
    }

    fn paint_backup(&self, ctx: &mut dyn FlowPaintCtx) {
        // Thumbnail: title + single rounded placeholder
        let px = self.position.x;
        let py = self.position.y;
        let bw = self.width;
        let bh = self.height;
        ctx.fill_rounded_rect(px, py, bw, bh, 10.0, [0.08, 0.14, 0.22, 1.0]);
        ctx.stroke_rounded_rect(px, py, bw, bh, 10.0, [0.4, 0.55, 0.72, 0.8], 1.5);
        ctx.draw_text_centered_in_rect(
            px,
            py + bh * 0.35,
            bw,
            bh * 0.3,
            &self.snapshot.blox_name,
            [0.85, 0.9, 1.0, 1.0],
            12.0,
        );
        ctx.draw_text_centered_in_rect(
            px,
            py + bh * 0.55,
            bw,
            bh * 0.25,
            "HSM (use paint for detail)",
            [0.55, 0.65, 0.78, 1.0],
            9.0,
        );
    }
}

/// Mix default [`RectNode`]s and [`HsmFlowNode`] on one Bloxflow canvas.
#[derive(Clone, Debug, PartialEq)]
pub enum BloxDiagramNode {
    Rect(RectNode),
    Hsm(HsmFlowNode),
}

impl BloxFlowNode for BloxDiagramNode {
    fn id(&self) -> &str {
        match self {
            BloxDiagramNode::Rect(n) => n.id(),
            BloxDiagramNode::Hsm(n) => n.id(),
        }
    }

    fn pos(&self) -> Position {
        match self {
            BloxDiagramNode::Rect(n) => n.pos(),
            BloxDiagramNode::Hsm(n) => n.pos(),
        }
    }

    fn set_pos(&mut self, p: Position) {
        match self {
            BloxDiagramNode::Rect(n) => n.set_pos(p),
            BloxDiagramNode::Hsm(n) => n.set_pos(p),
        }
    }

    fn width(&self) -> f32 {
        match self {
            BloxDiagramNode::Rect(n) => n.width(),
            BloxDiagramNode::Hsm(n) => n.width(),
        }
    }

    fn height(&self) -> f32 {
        match self {
            BloxDiagramNode::Rect(n) => n.height(),
            BloxDiagramNode::Hsm(n) => n.height(),
        }
    }

    fn selected(&self) -> bool {
        match self {
            BloxDiagramNode::Rect(n) => n.selected(),
            BloxDiagramNode::Hsm(n) => n.selected(),
        }
    }

    fn set_selected(&mut self, value: bool) {
        match self {
            BloxDiagramNode::Rect(n) => n.set_selected(value),
            BloxDiagramNode::Hsm(n) => n.set_selected(value),
        }
    }

    fn draggable(&self) -> bool {
        match self {
            BloxDiagramNode::Rect(n) => n.draggable(),
            BloxDiagramNode::Hsm(n) => n.draggable(),
        }
    }

    fn class_name(&self) -> &str {
        match self {
            BloxDiagramNode::Rect(n) => n.class_name(),
            BloxDiagramNode::Hsm(n) => n.class_name(),
        }
    }

    fn label(&self) -> &str {
        match self {
            BloxDiagramNode::Rect(n) => n.label(),
            BloxDiagramNode::Hsm(n) => n.label(),
        }
    }

    fn paint(&self, ctx: &mut dyn FlowPaintCtx) {
        match self {
            BloxDiagramNode::Rect(n) => n.paint(ctx),
            BloxDiagramNode::Hsm(n) => n.paint(ctx),
        }
    }

    fn paint_backup(&self, ctx: &mut dyn FlowPaintCtx) {
        match self {
            BloxDiagramNode::Rect(n) => n.paint_backup(ctx),
            BloxDiagramNode::Hsm(n) => n.paint_backup(ctx),
        }
    }
}
