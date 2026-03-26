// Copyright 2025 Bloxide, all rights reserved
//! Serializable-ish snapshot of one blox for visualization.
//!
//! Field names are stable so generated code and JSON (via the `serde` feature) stay aligned.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// One blox: states, transitions, optional implicit Init entry (engine-level).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BloxDiagramSnapshot {
    /// Stable id (crate/module path or actor name).
    pub blox_id: String,
    /// Human title in the UI.
    pub blox_name: String,
    pub states: Vec<StateSnapshot>,
    pub transitions: Vec<TransitionSnapshot>,
    /// When `Some`, an implicit **Init** node is drawn with an edge labeled `Start` into this state.
    pub implicit_entry_target: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StateSnapshot {
    /// Stable id (usually the Rust variant name, e.g. `Active`).
    pub id: String,
    /// Label on the canvas.
    pub display_name: String,
    pub kind: StateKindSnapshot,
    /// `None` for top-level states (under virtual root).
    pub parent_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StateKindSnapshot {
    Leaf,
    Composite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TransitionSnapshot {
    pub id: String,
    pub source_state_id: String,
    pub target_state_id: String,
    pub label: String,
    pub transition_kind: TransitionKindSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TransitionKindSnapshot {
    /// Domain / message-driven transition.
    DomainEvent,
    /// Lifecycle (Start, Reset, …) — informational for styling later.
    Lifecycle,
    /// Root-level fallback rules.
    RootFallback,
}

/// Example snapshot mirroring the **Ping** blox topology and plausible transitions (hand-authored).
///
/// Replace with macro-generated data once `MachineSpec` introspection exists.
pub fn example_ping_blox_snapshot() -> BloxDiagramSnapshot {
    BloxDiagramSnapshot {
        blox_id: "ping_blox".into(),
        blox_name: "Ping blox (example snapshot)".into(),
        states: vec![
            StateSnapshot {
                id: "Operating".into(),
                display_name: "Operating".into(),
                kind: StateKindSnapshot::Composite,
                parent_id: None,
            },
            StateSnapshot {
                id: "Active".into(),
                display_name: "Active".into(),
                kind: StateKindSnapshot::Leaf,
                parent_id: Some("Operating".into()),
            },
            StateSnapshot {
                id: "Paused".into(),
                display_name: "Paused".into(),
                kind: StateKindSnapshot::Leaf,
                parent_id: Some("Operating".into()),
            },
            StateSnapshot {
                id: "Done".into(),
                display_name: "Done".into(),
                kind: StateKindSnapshot::Leaf,
                parent_id: None,
            },
            StateSnapshot {
                id: "Error".into(),
                display_name: "Error".into(),
                kind: StateKindSnapshot::Leaf,
                parent_id: None,
            },
        ],
        transitions: vec![
            TransitionSnapshot {
                id: "t_active_paused".into(),
                source_state_id: "Active".into(),
                target_state_id: "Paused".into(),
                label: "pause threshold".into(),
                transition_kind: TransitionKindSnapshot::DomainEvent,
            },
            TransitionSnapshot {
                id: "t_paused_active".into(),
                source_state_id: "Paused".into(),
                target_state_id: "Active".into(),
                label: "Resume".into(),
                transition_kind: TransitionKindSnapshot::DomainEvent,
            },
            TransitionSnapshot {
                id: "t_active_done".into(),
                source_state_id: "Active".into(),
                target_state_id: "Done".into(),
                label: "max rounds".into(),
                transition_kind: TransitionKindSnapshot::DomainEvent,
            },
            TransitionSnapshot {
                id: "t_active_error".into(),
                source_state_id: "Active".into(),
                target_state_id: "Error".into(),
                label: "fault".into(),
                transition_kind: TransitionKindSnapshot::DomainEvent,
            },
        ],
        implicit_entry_target: Some("Active".into()),
    }
}
