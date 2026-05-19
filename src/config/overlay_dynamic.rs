//! Dynamic-section helpers for the overlay merge: comment-preserving
//! `add` / `remove` / `reorder` on `[[array-of-tables]]` blocks introduced
//! by issue #790.
//!
//! Sub-table handling for dynamic-keyed maps (`[agents.<id>]`, `[modes.<id>]`,
//! `[teams.<id>]`) already works through the existing `merge_table` recursion
//! and the wildcard `target.insert` arm in `merge_key`. The pain point this
//! module fixes is `Item::ArrayOfTables`: the old wildcard arm rewrote the
//! whole array on any change, destroying per-element prefix comments.
//!
//! Reorder is feature-gated behind `dynamic-config-reorder` (off-by-default
//! for one release). Append, remove, and per-slot edits ship unconditionally;
//! they preserve neighbor comments by mutating only the affected slots.

use std::collections::{BTreeMap, VecDeque};

use toml_edit::{ArrayOfTables, Item, Table, Value};

const REORDER_ENABLED: bool = cfg!(feature = "dynamic-config-reorder");

/// Merge `new_arr` into `target_arr` using `old_arr` as the on-disk view.
/// Detects per-element identity by canonical content hash, so a swap of two
/// neighbours is recognised as a reorder rather than two unrelated edits.
pub(super) fn merge_array_of_tables(
    target_arr: &mut ArrayOfTables,
    old_arr: &ArrayOfTables,
    new_arr: &ArrayOfTables,
) {
    let perm = build_permutation(old_arr, new_arr);
    let reorder_needed = perm
        .iter()
        .enumerate()
        .any(|(new_idx, src)| matches!(src, Source::Existing(old_idx) if *old_idx != new_idx));

    if reorder_needed && !REORDER_ENABLED {
        tracing::debug!(
            target: "config.overlay",
            "array-of-tables reorder: dynamic-config-reorder feature is off — \
             falling back to wholesale rewrite (moved-row comments lost)",
        );
        let fresh: Vec<Source> = (0..new_arr.len()).map(Source::Fresh).collect();
        apply_permutation(target_arr, new_arr, &fresh);
        return;
    }

    apply_permutation(target_arr, new_arr, &perm);
}

#[derive(Debug, Clone, Copy)]
enum Source {
    /// Pull from `target_arr` at this old-slot index (preserves decor).
    Existing(usize),
    /// Pull from `new_arr` at this new-slot index (fresh element, no decor
    /// to preserve).
    Fresh(usize),
}

/// FIFO-bucket match: each new element claims the first unmatched old
/// element with an equal canonical hash. Unmatched new elements are `Fresh`;
/// unmatched old elements are dropped (i.e. removed).
fn build_permutation(old_arr: &ArrayOfTables, new_arr: &ArrayOfTables) -> Vec<Source> {
    let mut buckets: BTreeMap<String, VecDeque<usize>> = BTreeMap::new();
    for i in 0..old_arr.len() {
        buckets
            .entry(canon_table(old_arr.get(i)))
            .or_default()
            .push_back(i);
    }
    let mut out = Vec::with_capacity(new_arr.len());
    for j in 0..new_arr.len() {
        let claimed = buckets
            .get_mut(&canon_table(new_arr.get(j)))
            .and_then(VecDeque::pop_front);
        out.push(match claimed {
            Some(i) => Source::Existing(i),
            None => Source::Fresh(j),
        });
    }
    out
}

fn apply_permutation(target_arr: &mut ArrayOfTables, new_arr: &ArrayOfTables, perm: &[Source]) {
    let snapshot: Vec<Table> = (0..target_arr.len())
        .map(|i| target_arr.get(i).cloned().expect("len-checked snapshot"))
        .collect();
    let base_position = snapshot
        .iter()
        .filter_map(Table::position)
        .min()
        .unwrap_or(0);
    target_arr.clear();
    for (offset, src) in perm.iter().enumerate() {
        let mut table = match src {
            Source::Existing(i) => snapshot[*i].clone(),
            Source::Fresh(j) => match new_arr.get(*j) {
                Some(t) => t.clone(),
                None => continue,
            },
        };
        table.set_position(base_position.saturating_add(offset));
        target_arr.push(table);
    }
}

/// Canonical, decor-blind, key-sorted serialization used as a content hash.
/// Two tables with identical primitive payloads canonicalize to the same
/// string regardless of insertion order or surrounding comments.
fn canon_table(table: Option<&Table>) -> String {
    let Some(t) = table else { return String::new() };
    let mut entries: Vec<(String, String)> = Vec::new();
    for (key, item) in t.iter() {
        entries.push((key.to_string(), canon_item(item)));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::with_capacity(64);
    out.push('{');
    for (k, v) in &entries {
        out.push_str(k);
        out.push('=');
        out.push_str(v);
        out.push(';');
    }
    out.push('}');
    out
}

fn canon_item(item: &Item) -> String {
    match item {
        Item::None => String::new(),
        Item::Value(v) => canon_value(v),
        Item::Table(t) => canon_table(Some(t)),
        Item::ArrayOfTables(arr) => {
            let mut s = String::from("[");
            for i in 0..arr.len() {
                s.push_str(&canon_table(arr.get(i)));
                s.push(',');
            }
            s.push(']');
            s
        }
    }
}

fn canon_value(value: &Value) -> String {
    let mut clone = value.clone();
    clone.decor_mut().clear();
    clone.to_string().trim().to_string()
}
