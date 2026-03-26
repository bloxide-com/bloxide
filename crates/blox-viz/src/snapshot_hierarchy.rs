// Copyright 2025 Bloxide, all rights reserved
//! Shared parent/child grouping for diagram backends.

use std::collections::BTreeMap;

use crate::snapshot::StateSnapshot;

pub(crate) fn group_children_by_parent(
    states: &[StateSnapshot],
) -> BTreeMap<Option<String>, Vec<&StateSnapshot>> {
    let mut map: BTreeMap<Option<String>, Vec<&StateSnapshot>> = BTreeMap::new();
    for s in states {
        map.entry(s.parent_id.clone()).or_default().push(s);
    }
    for v in map.values_mut() {
        v.sort_by(|a, b| a.id.cmp(&b.id));
    }
    map
}

pub(crate) fn sort_root_states<'a>(
    roots: &mut Vec<&'a StateSnapshot>,
    by_parent: &BTreeMap<Option<String>, Vec<&'a StateSnapshot>>,
) {
    roots.sort_by(|a, b| {
        let a_composite = by_parent.contains_key(&Some(a.id.clone()));
        let b_composite = by_parent.contains_key(&Some(b.id.clone()));
        b_composite
            .cmp(&a_composite)
            .then_with(|| a.id.cmp(&b.id))
    });
}
