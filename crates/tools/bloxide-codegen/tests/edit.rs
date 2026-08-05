// Copyright 2025 Bloxide, all rights reserved
//! Tests for the feature-gate-aware transition edit primitives
//! (`bloxide_codegen::edit::add_transition` / `remove_transition`).
//!
//! A transition's identity is the triple `(state, event, feature)`: a
//! feature-gated variant of an existing `(state, event)` rule is a distinct
//! entry, not a duplicate.

use bloxide_codegen::edit::{self, EditError};
use toml_edit::DocumentMut;

/// A blox.toml document with one non-gated transition `Idle + Ping -> Active`.
const FIXTURE_NON_GATED: &str = "\
[topology]

[[topology.states]]
name = \"Idle\"

[[topology.states]]
name = \"Active\"

[[topology.transitions]]
state = \"Idle\"
event = \"TestMsg::Ping(_)\"
target = \"Active\"
";

/// Same (state, event) pair twice: once non-gated, once gated on `dynamic`.
const FIXTURE_GATED_PAIR: &str = "\
[topology]

[[topology.states]]
name = \"Idle\"

[[topology.states]]
name = \"Active\"

[[topology.transitions]]
state = \"Idle\"
event = \"TestMsg::Ping(_)\"
target = \"Active\"

[[topology.transitions]]
state = \"Idle\"
event = \"TestMsg::Ping(_)\"
target = \"Active\"
feature = \"dynamic\"
";

fn parse(src: &str) -> DocumentMut {
    src.parse::<DocumentMut>().expect("fixture parses")
}

/// (state, event, feature) triples of every transition in the document.
fn transition_keys(doc: &DocumentMut) -> Vec<(String, String, Option<String>)> {
    doc["topology"]["transitions"]
        .as_array_of_tables()
        .expect("transitions is an array of tables")
        .iter()
        .map(|t| {
            (
                t["state"].as_str().expect("state").to_string(),
                t["event"].as_str().expect("event").to_string(),
                t.get("feature")
                    .and_then(|f| f.as_str())
                    .map(str::to_string),
            )
        })
        .collect()
}

#[test]
fn add_feature_gated_variant_of_non_gated_succeeds() {
    let mut doc = parse(FIXTURE_NON_GATED);
    edit::add_transition(
        &mut doc,
        "Idle",
        "TestMsg::Ping(_)",
        "Active",
        vec![],
        vec![],
        Some("dynamic"),
    )
    .expect("feature-gated variant is not a duplicate");

    assert_eq!(
        transition_keys(&doc),
        vec![
            ("Idle".to_string(), "TestMsg::Ping(_)".to_string(), None),
            (
                "Idle".to_string(),
                "TestMsg::Ping(_)".to_string(),
                Some("dynamic".to_string())
            ),
        ]
    );
}

#[test]
fn add_exact_duplicate_still_conflicts() {
    // Same (state, event, feature) — including both non-gated.
    let mut doc = parse(FIXTURE_NON_GATED);
    let err = edit::add_transition(
        &mut doc,
        "Idle",
        "TestMsg::Ping(_)",
        "Active",
        vec![],
        vec![],
        None,
    )
    .expect_err("non-gated duplicate must conflict");
    assert!(
        matches!(
            err.downcast_ref::<EditError>(),
            Some(EditError::Conflict(_))
        ),
        "expected Conflict, got: {err}"
    );

    // Same (state, event) with the same feature gate.
    let mut doc = parse(FIXTURE_GATED_PAIR);
    let err = edit::add_transition(
        &mut doc,
        "Idle",
        "TestMsg::Ping(_)",
        "Active",
        vec![],
        vec![],
        Some("dynamic"),
    )
    .expect_err("feature-gated duplicate must conflict");
    assert!(
        matches!(
            err.downcast_ref::<EditError>(),
            Some(EditError::Conflict(_))
        ),
        "expected Conflict, got: {err}"
    );
}

#[test]
fn remove_feature_gated_variant_leaves_non_gated() {
    let mut doc = parse(FIXTURE_GATED_PAIR);
    edit::remove_transition(&mut doc, "Idle", "TestMsg::Ping(_)", Some("dynamic"))
        .expect("gated variant exists");

    assert_eq!(
        transition_keys(&doc),
        vec![("Idle".to_string(), "TestMsg::Ping(_)".to_string(), None)],
        "only the non-gated transition remains"
    );
}

#[test]
fn remove_without_feature_leaves_gated_variant() {
    let mut doc = parse(FIXTURE_GATED_PAIR);
    edit::remove_transition(&mut doc, "Idle", "TestMsg::Ping(_)", None)
        .expect("non-gated variant exists");

    assert_eq!(
        transition_keys(&doc),
        vec![(
            "Idle".to_string(),
            "TestMsg::Ping(_)".to_string(),
            Some("dynamic".to_string())
        )],
        "only the feature-gated transition remains"
    );
}

#[test]
fn remove_missing_feature_variant_is_not_found() {
    // The non-gated transition exists, but no `dynamic`-gated variant.
    let mut doc = parse(FIXTURE_NON_GATED);
    let err = edit::remove_transition(&mut doc, "Idle", "TestMsg::Ping(_)", Some("dynamic"))
        .expect_err("no gated variant to remove");
    assert!(
        matches!(
            err.downcast_ref::<EditError>(),
            Some(EditError::NotFound(_))
        ),
        "expected NotFound, got: {err}"
    );
    assert_eq!(
        transition_keys(&doc).len(),
        1,
        "nothing was removed on a miss"
    );
}

#[test]
fn add_state_rejects_composite_and_error_together() {
    // StateKind is Leaf | Composite | Error — the flags are mutually
    // exclusive, so setting both must be an error, not a silent pick.
    let mut doc = parse(FIXTURE_NON_GATED);
    let err = edit::add_state(&mut doc, "Boom", None, true, true)
        .expect_err("composite + error must be rejected");
    assert!(
        err.to_string().contains("both composite and error"),
        "unexpected error: {err}"
    );

    // Each flag on its own is fine.
    let mut doc = parse(FIXTURE_NON_GATED);
    edit::add_state(&mut doc, "Comp", None, true, false).expect("composite alone is ok");
    edit::add_state(&mut doc, "Failed", None, false, true).expect("error alone is ok");
}

#[test]
fn add_guard_with_valid_targets_succeeds() {
    let mut doc = parse(FIXTURE_NON_GATED);
    edit::add_transition(
        &mut doc,
        "Idle",
        "TestMsg::Tick(_)",
        "Active",
        vec![],
        vec![
            "ctx.count > 3:Active".to_string(), // state-name target
            "ctx.count > 9:stop".to_string(),   // keyword target
        ],
        None,
    )
    .expect("valid guard targets are accepted");
}

#[test]
fn add_guard_with_colon_in_string_literal_is_a_clear_error() {
    // `ctx.name == "a:b"` omits the `:target` suffix; the LAST-':' split
    // lands inside the string literal, yielding the bogus target `b"`.
    // That must be a clear error, not a silently accepted mis-split.
    let mut doc = parse(FIXTURE_NON_GATED);
    let err = edit::add_transition(
        &mut doc,
        "Idle",
        "TestMsg::Tick(_)",
        "Active",
        vec![],
        vec![r#"ctx.name == "a:b""#.to_string()],
        None,
    )
    .expect_err("mis-split guard target must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("target 'b\"'") && msg.contains("string literals"),
        "error should name the bad target and the literal caveat, got: {msg}"
    );
}

#[test]
fn add_guard_with_unknown_target_state_is_an_error() {
    // Same validation path, without any colon subtlety: a target that is
    // neither a declared state nor a keyword is rejected.
    let mut doc = parse(FIXTURE_NON_GATED);
    let err = edit::add_transition(
        &mut doc,
        "Idle",
        "TestMsg::Tick(_)",
        "Active",
        vec![],
        vec!["ctx.count > 3:Nowhere".to_string()],
        None,
    )
    .expect_err("unknown guard target must be rejected");
    assert!(
        err.to_string().contains("target 'Nowhere'"),
        "unexpected error: {err}"
    );
}
