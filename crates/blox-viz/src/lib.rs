// Copyright 2025 Bloxide, all rights reserved
//! Static **diagram snapshots** for a single blox, convertible to [`bloxflow_core`] nodes and edges,
//! SVG/PNG via [`HsmFlowNode`] paint (same as Bloxflow canvas), plus Graphviz DOT and Mermaid.
//!
//! Today snapshots are built by hand or from examples. Later, Bloxide proc macros can emit
//! [`BloxDiagramSnapshot`] (or `impl From<YourSpec> for BloxDiagramSnapshot`) from `MachineSpec`
//! / `StateTopology` metadata.
//!
//! For **Bloxflow** HTML canvas, use [`HsmFlowNode`] / [`BloxDiagramNode`] (implements
//! [`bloxflow_core::BloxFlowNode`]) to draw a blox as one nested widget.

#![forbid(unsafe_code)]

mod dot;
mod hsm_flow_node;
mod layout;
mod mermaid;
#[cfg(feature = "png")]
mod raster;
mod snapshot;
mod snapshot_hierarchy;
mod svg;

pub use dot::snapshot_to_dot;
pub use hsm_flow_node::{BloxDiagramNode, HsmFlowNode};
pub use layout::{snapshot_to_flow, state_depths, UML_INIT_DOT_D};
pub use mermaid::snapshot_to_mermaid;
#[cfg(feature = "png")]
pub use raster::{snapshot_to_png, RasterizeError};
pub use snapshot::{
    example_ping_blox_snapshot, BloxDiagramSnapshot, StateKindSnapshot, StateSnapshot,
    TransitionKindSnapshot, TransitionSnapshot,
};
pub use svg::{snapshot_to_svg, SvgRenderConfig};
