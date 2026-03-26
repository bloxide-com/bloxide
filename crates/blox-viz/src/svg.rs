// Copyright 2025 Bloxide, all rights reserved
//! SVG export through the same paint path as the Bloxflow canvas: [`crate::hsm_flow_node::HsmFlowNode`]
//! and [`bloxflow_core::FlowPaintCtx`]. One nested **widget** (chrome + title + inner machine), not a
//! flat spread of separate SVG state nodes.

use bloxflow_core::{BloxFlowNode, FlowPaintCtx, Position, Viewport};

use crate::hsm_flow_node::HsmFlowNode;
use crate::snapshot::BloxDiagramSnapshot;

/// Matches the embedded TTF loaded in [`crate::raster::snapshot_to_png`] for reliable labels.
pub(crate) const EXPORT_FONT_FAMILY: &str = "DejaVu Sans";

/// Canvas size and padding for [`snapshot_to_svg`].
#[derive(Clone, Copy, Debug)]
pub struct SvgRenderConfig {
    pub width: f32,
    pub height: f32,
    pub padding: f32,
    /// Vertical space reserved for the title below the top edge.
    pub title_band: f32,
}

impl Default for SvgRenderConfig {
    fn default() -> Self {
        Self {
            width: 1180.0,
            height: 720.0,
            padding: 36.0,
            title_band: 72.0,
        }
    }
}

/// Full SVG document (`width` / `height` match `cfg`).
pub fn snapshot_to_svg(snapshot: &BloxDiagramSnapshot, cfg: SvgRenderConfig) -> String {
    let drawable_h = (cfg.height - cfg.title_band).max(120.0);
    let inner_x = cfg.padding;
    let inner_y = cfg.title_band + 8.0;
    let inner_w = (cfg.width - 2.0 * cfg.padding).max(120.0);
    let inner_h = (drawable_h - cfg.padding - 8.0).max(100.0);

    let node = HsmFlowNode::new(
        "blox_export",
        snapshot.clone(),
        Position::new(0.0, 0.0),
        inner_w,
        inner_h,
    )
    .with_draggable(false);

    let view = Viewport {
        x: inner_x,
        y: inner_y,
        zoom: 1.0,
    };

    let mut painted = String::new();
    let mut paint_ctx = SvgFlowPaintCtx {
        view,
        out: &mut painted,
        ff: EXPORT_FONT_FAMILY,
    };
    node.paint(&mut paint_ctx);

    let mut defs = String::new();
    let bg_id = "bf_bg_grad";
    defs.push_str(&format!(
        r#"<radialGradient id="{bg_id}" cx="20%" cy="0%" r="85%">
  <stop offset="0%" stop-color='#152238'/>
  <stop offset="55%" stop-color='#0a0f18'/>
  <stop offset="100%" stop-color='#06090f'/>
</radialGradient>
"#
    ));

    let bg_fill = format!("url(#{})", bg_id);
    let mut body = String::new();
    body.push_str(&format!(
        r#"<rect x="0" y="0" width="{w}" height="{h}" fill="{bg_fill}" rx="14"/>"#,
        w = cfg.width,
        h = cfg.height,
        bg_fill = bg_fill,
    ));
    body.push_str(&format!(
        r#"<text x="{cx}" y="32" text-anchor="middle" fill='#e8f1ff' font-family='{ff}' font-size="18" font-weight="bold">{title}</text>"#,
        title = escape_xml(&snapshot.blox_name),
        cx = cfg.width * 0.5,
        ff = EXPORT_FONT_FAMILY,
    ));
    body.push_str(&format!(
        r#"<text x="{cx}" y="54" text-anchor="middle" fill='#8fa6c2' font-family='{ff}' font-size="11">Bloxflow HSM widget — same paint as HTML canvas (nested chrome, scaled graph, straight transitions)</text>"#,
        cx = cfg.width * 0.5,
        ff = EXPORT_FONT_FAMILY,
    ));
    body.push_str(r#"<g id="blox-hsm-widget">"#);
    body.push_str(&painted);
    body.push_str("</g>");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
<defs>
{defs}</defs>
{body}
</svg>
"#,
        w = cfg.width,
        h = cfg.height,
        defs = defs,
        body = body
    )
}

struct SvgFlowPaintCtx<'a> {
    view: Viewport,
    out: &'a mut String,
    ff: &'static str,
}

impl SvgFlowPaintCtx<'_> {
    fn to_screen(&self, gx: f32, gy: f32) -> (f32, f32) {
        (
            self.view.x + gx * self.view.zoom,
            self.view.y + gy * self.view.zoom,
        )
    }

    fn rgba_attr(c: [f32; 4]) -> String {
        format!(
            "rgba({},{},{},{})",
            (c[0] * 255.0).round() as i32,
            (c[1] * 255.0).round() as i32,
            (c[2] * 255.0).round() as i32,
            c[3]
        )
    }
}

impl FlowPaintCtx for SvgFlowPaintCtx<'_> {
    fn zoom(&self) -> f32 {
        self.view.zoom
    }

    fn fill_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        rgba: [f32; 4],
    ) {
        let (sx, sy) = self.to_screen(x, y);
        let sw = w * self.view.zoom;
        let sh = h * self.view.zoom;
        let r = (radius * self.view.zoom)
            .min(sw * 0.5)
            .min(sh * 0.5)
            .max(0.0);
        let fill = Self::rgba_attr(rgba);
        self.out.push_str(&format!(
            r#"<rect x="{sx:.2}" y="{sy:.2}" width="{sw:.2}" height="{sh:.2}" rx="{r:.2}" ry="{r:.2}" fill="{fill}"/>"#,
        ));
    }

    fn stroke_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        rgba: [f32; 4],
        stroke_width: f32,
    ) {
        let (sx, sy) = self.to_screen(x, y);
        let sw = w * self.view.zoom;
        let sh = h * self.view.zoom;
        let r = (radius * self.view.zoom)
            .min(sw * 0.5)
            .min(sh * 0.5)
            .max(0.0);
        let stroke = Self::rgba_attr(rgba);
        let lw = (stroke_width * self.view.zoom.max(0.05)).max(0.25);
        self.out.push_str(&format!(
            r#"<rect x="{sx:.2}" y="{sy:.2}" width="{sw:.2}" height="{sh:.2}" rx="{r:.2}" ry="{r:.2}" fill="none" stroke="{stroke}" stroke-width="{lw:.2}"/>"#,
        ));
    }

    fn draw_text_centered_in_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &str,
        rgba: [f32; 4],
        font_size_graph: f32,
    ) {
        let (sx, sy) = self.to_screen(x, y);
        let sw = w * self.view.zoom;
        let sh = h * self.view.zoom;
        let px = (font_size_graph * self.view.zoom).clamp(9.0, 22.0);
        let cx = sx + sw * 0.5;
        let cy = sy + sh * 0.5;
        let fill = Self::rgba_attr(rgba);
        let t = escape_xml(text);
        self.out.push_str(&format!(
            r#"<text x="{cx:.2}" y="{cy:.2}" dominant-baseline="middle" text-anchor="middle" fill="{fill}" font-family="{ff}" font-size="{px:.1}" font-weight="500">{t}</text>"#,
            ff = self.ff,
        ));
    }

    fn stroke_line(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        rgba: [f32; 4],
        stroke_width: f32,
    ) {
        let (sx0, sy0) = self.to_screen(x0, y0);
        let (sx1, sy1) = self.to_screen(x1, y1);
        let stroke = Self::rgba_attr(rgba);
        let lw = (stroke_width * self.view.zoom.max(0.05)).max(0.25);
        self.out.push_str(&format!(
            r#"<line x1="{sx0:.2}" y1="{sy0:.2}" x2="{sx1:.2}" y2="{sy1:.2}" stroke="{stroke}" stroke-width="{lw:.2}" stroke-linecap="round"/>"#,
        ));
    }
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::example_ping_blox_snapshot;

    #[test]
    fn ping_svg_is_well_formed_widget_export() {
        let svg = snapshot_to_svg(&example_ping_blox_snapshot(), SvgRenderConfig::default());
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("<svg "));
        assert!(svg.contains("Ping"));
        assert!(svg.contains("Operating"));
        assert!(svg.contains(r#"id="blox-hsm-widget""#));
        assert!(
            svg.contains("Bloxflow HSM widget"),
            "subtitle should describe widget export"
        );
        assert!(
            !svg.contains("blox-machine-frame"),
            "flat UML machine frame is not used for widget export"
        );
        assert!(
            svg.contains("<line ") && svg.contains("stroke-linecap=\"round\""),
            "transitions should be straight segments like canvas paint"
        );
    }
}
