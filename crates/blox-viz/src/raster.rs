// Copyright 2025 Bloxide, all rights reserved
//! Rasterize [`crate::svg::snapshot_to_svg`] (HSM widget SVG) with [resvg](https://github.com/linebender/resvg).

use resvg::tiny_skia::Pixmap;
use resvg::usvg;

use crate::snapshot::BloxDiagramSnapshot;
use crate::svg::{snapshot_to_svg, SvgRenderConfig};

/// PNG rasterization failed (SVG parse or encode).
#[derive(Debug)]
pub struct RasterizeError(String);

impl std::fmt::Display for RasterizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for RasterizeError {}

impl From<usvg::Error> for RasterizeError {
    fn from(e: usvg::Error) -> Self {
        RasterizeError(e.to_string())
    }
}

/// Encode the diagram as PNG (sRGB), using the same layout as the SVG export.
pub fn snapshot_to_png(
    snapshot: &BloxDiagramSnapshot,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, RasterizeError> {
    let cfg = SvgRenderConfig {
        width: width as f32,
        height: height as f32,
        ..SvgRenderConfig::default()
    };
    let svg = snapshot_to_svg(snapshot, cfg);

    let mut opt = usvg::Options::default();
    let db = opt.fontdb_mut();
    db.load_font_data(include_bytes!("../fonts/DejaVuSans.ttf").to_vec());
    db.load_system_fonts();
    opt.font_family = crate::svg::EXPORT_FONT_FAMILY.to_string();

    let tree = usvg::Tree::from_str(&svg, &opt)?;
    let size = tree.size().to_int_size();
    let w = size.width();
    let h = size.height();
    let mut pixmap = Pixmap::new(w, h).ok_or_else(|| RasterizeError("empty pixmap size".into()))?;

    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );

    pixmap
        .encode_png()
        .map_err(|e| RasterizeError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::example_ping_blox_snapshot;

    #[test]
    fn ping_example_png_header() {
        let png = snapshot_to_png(&example_ping_blox_snapshot(), 800, 520).expect("png");
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    }
}
