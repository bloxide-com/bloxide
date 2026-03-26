// Copyright 2025 Bloxide, all rights reserved
//! Emit Mermaid, Graphviz DOT, SVG, or PNG (PNG via [resvg](https://github.com/linebender/resvg)).

use std::fs;
use std::path::{Path, PathBuf};

use blox_viz::{
    example_ping_blox_snapshot, snapshot_to_dot, snapshot_to_mermaid, snapshot_to_png,
    snapshot_to_svg, BloxDiagramSnapshot, SvgRenderConfig,
};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "blox-diagram",
    about = "Blox diagram: snapshot → Mermaid, DOT, SVG, or PNG (resvg; no Node)"
)]
struct Args {
    /// JSON file (`BloxDiagramSnapshot`, `blox-viz` feature `serde`).
    #[arg(short, long, value_name = "FILE")]
    input: Option<PathBuf>,

    /// Built-in snapshot (for demos).
    #[arg(long, value_name = "NAME")]
    example: Option<String>,

    /// Write diagram text. Extension picks format: `.svg`, `.dot`/`.gv`, `.mmd`/`.mermaid` → else Mermaid.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Rasterize styled SVG to PNG (uses `--width` / `--height`).
    #[arg(long, value_name = "FILE")]
    png: Option<PathBuf>,

    /// Canvas width for `--png` and for `.svg` output.
    #[arg(long, default_value_t = 1180)]
    width: u32,

    /// Canvas height for `--png` and for `.svg` output.
    #[arg(long, default_value_t = 720)]
    height: u32,
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let snapshot = load_snapshot(&args)?;

    let mmd = snapshot_to_mermaid(&snapshot);
    let dot = snapshot_to_dot(&snapshot);
    let svg_cfg = SvgRenderConfig {
        width: args.width as f32,
        height: args.height as f32,
        ..SvgRenderConfig::default()
    };
    let svg = snapshot_to_svg(&snapshot, svg_cfg);

    match &args.output {
        Some(p) if is_dot_ext(p) => {
            fs::write(p, dot.as_bytes()).map_err(|e| e.to_string())?;
        }
        Some(p) if is_svg_ext(p) => {
            fs::write(p, svg.as_bytes()).map_err(|e| e.to_string())?;
        }
        Some(p) => {
            fs::write(p, mmd.as_bytes()).map_err(|e| e.to_string())?;
        }
        None => {}
    }

    if let Some(png) = &args.png {
        let png_bytes = snapshot_to_png(&snapshot, args.width, args.height)
            .map_err(|e| e.to_string())?;
        fs::write(png, png_bytes).map_err(|e| e.to_string())?;
    } else if args.output.is_none() {
        print!("{mmd}");
    }

    Ok(())
}

fn is_dot_ext(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|s| s.to_str()),
        Some("dot" | "gv")
    )
}

fn is_svg_ext(p: &Path) -> bool {
    matches!(p.extension().and_then(|s| s.to_str()), Some("svg"))
}

fn load_snapshot(args: &Args) -> Result<BloxDiagramSnapshot, String> {
    match (&args.input, &args.example) {
        (Some(path), None) => {
            let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
            serde_json::from_str(&raw).map_err(|e| format!("JSON: {e}"))
        }
        (None, Some(name)) => match name.as_str() {
            "ping" => Ok(example_ping_blox_snapshot()),
            other => Err(format!("unknown --example {other:?} (try: ping)")),
        },
        (Some(_), Some(_)) => Err("use either --input or --example, not both".into()),
        (None, None) => Err("pass --input FILE.json or --example ping".into()),
    }
}
