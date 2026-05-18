//! TOML overlay merge: apply changes from a serialized `Config` onto an
//! existing `toml_edit::DocumentMut` while preserving comments, blank lines,
//! key order, and unknown sections that are not modeled by the `Config`
//! struct.
//!
//! Algorithm:
//!   - The caller serializes both the in-memory `Config` and the parsed-on-disk
//!     `Config` with `toml::to_string`, then re-parses each into a
//!     `DocumentMut`. Those canonical-form docs share identical decor for
//!     semantically equal values, so they can be diffed by direct comparison.
//!   - This module walks `new` (the in-memory canonical doc) and `on_disk`
//!     (the parsed-on-disk canonical doc) in lockstep against `target` (the
//!     existing file's `DocumentMut`). For each key:
//!       - if `new` and `on_disk` agree deeply, the field is unchanged (or a
//!         default the user never spelled out). Leave `target` alone — this
//!         is what preserves decor and also avoids inserting phantom defaults.
//!       - if a primitive value differs, swap only the inner `Value` on the
//!         existing item while preserving its surrounding decor (this keeps
//!         adjacent comments intact across a single-key edit).
//!       - if both sides are tables, recurse.
//!       - any other shape change (type swap, new key, array-of-tables
//!         replacement) is wholesale.
//!   - Keys present in `on_disk` but absent in `new` have been removed from
//!     `self` — drop them from `target`. Keys in `target` that appear in
//!     neither canonical doc are unknown sections — leave them alone.

use toml_edit::{DocumentMut, Item, Table, Value};

pub(super) fn apply_overlay(existing: &mut DocumentMut, on_disk: &Table, new: &Table) {
    merge_table(existing.as_table_mut(), on_disk, new);
}

fn merge_table(target: &mut Table, on_disk: &Table, new: &Table) {
    for (key, new_item) in new.iter() {
        merge_key(target, key, on_disk.get(key), new_item);
    }

    let new_keys: std::collections::HashSet<&str> = new.iter().map(|(k, _)| k).collect();
    let removed: Vec<String> = on_disk
        .iter()
        .filter(|(k, _)| !new_keys.contains(k))
        .map(|(k, _)| k.to_string())
        .collect();
    for k in removed {
        if target.contains_key(&k) {
            target.remove(&k);
        }
    }
}

fn merge_key(target: &mut Table, key: &str, old: Option<&Item>, new: &Item) {
    // First: deep-equality short circuit. If the in-memory and on-disk
    // versions agree, the field is unchanged (or a default that the user
    // never spelled out). Leave the target alone so we neither mutate its
    // decor nor insert a phantom default section.
    if let Some(old_item) = old
        && deep_eq(old_item, new)
    {
        return;
    }

    match (old, new) {
        (Some(Item::Table(old_tbl)), Item::Table(new_tbl)) => {
            // Recurse into the table — its content has at least one change.
            if let Some(target_item) = target.get_mut(key)
                && let Some(target_tbl) = target_item.as_table_mut()
            {
                merge_table(target_tbl, old_tbl, new_tbl);
                return;
            }
            target.insert(key, new.clone());
        }
        (Some(Item::Value(_)), Item::Value(new_val)) => {
            replace_value_preserving_decor(target, key, new_val.clone());
        }
        _ => {
            target.insert(key, new.clone());
        }
    }
}

fn deep_eq(a: &Item, b: &Item) -> bool {
    // Compare by wrapping each Item under a fresh root key in a new document.
    // The wrapped serialization captures all nested sub-tables, which a bare
    // `Item::to_string()` would drop for `Item::Table` (sub-tables are
    // emitted by the parent DocumentMut, not by the Table itself).
    fn render(item: &Item) -> String {
        let mut doc = DocumentMut::new();
        doc.as_table_mut().insert("__root", item.clone());
        doc.to_string()
    }
    render(a) == render(b)
}

fn replace_value_preserving_decor(target: &mut Table, key: &str, new_val: Value) {
    if let Some(existing_item) = target.get_mut(key)
        && let Some(existing_val) = existing_item.as_value_mut()
    {
        let mut new_val = new_val;
        copy_decor(existing_val, &mut new_val);
        *existing_val = new_val;
        return;
    }
    target.insert(key, Item::Value(new_val));
}

fn copy_decor(src: &Value, dst: &mut Value) {
    let prefix = src.decor().prefix().cloned();
    let suffix = src.decor().suffix().cloned();
    if let Some(p) = prefix {
        dst.decor_mut().set_prefix(p);
    }
    if let Some(s) = suffix {
        dst.decor_mut().set_suffix(s);
    }
}
