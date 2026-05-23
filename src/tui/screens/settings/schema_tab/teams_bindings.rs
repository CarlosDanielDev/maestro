//! Adapter for `[teams.<id>].bindings` — the only `#[serde(flatten)]` map
//! in the dynamic-config registry.
//!
//! The asymmetry the adapter resolves:
//!
//! * **On disk** (`TeamConfig.bindings: #[serde(flatten)] HashMap<String, toml::Value>`):
//!   role → agent pairs are top-level scalar keys on `[teams.<id>]`, e.g.
//!   `coder = "claude"` sitting next to `extends`, `primitive`, etc.
//! * **In the TUI**: a single `StringList` field named `bindings` whose items
//!   look like `"role=agent"`.
//!
//! [`collapse_team_bindings_into_array`] runs on the build path so the
//! [`EntryState`](super::widgets::entry_state::EntryState) builder finds a
//! `bindings = [...]` array where its `StringList` field expects one. The
//! inverse [`explode_bindings_array_to_top_level`] runs on the sync path so
//! the saved TOML stays flat (which is what `TeamConfig`'s `#[serde(flatten)]`
//! expects on the next reload).
//!
//! Schema lock: this is the ONLY consumer of the flat ↔ array reshape. Spec
//! `docs/superpowers/specs/2026-05-19-dynamic-config-editing.md` §6.3 + §7.
//! A generic capability on `FieldKind::FlattenedMap` would be over-engineering
//! for one consumer.

pub(crate) const TEAMS_SECTION_PATH: &str = "teams";

/// Schema keys reserved by `TEAMS_ENTRY_FIELDS` (plus `role_overrides`, which
/// is deferred but already a recognised non-binding sibling on disk). The
/// decoder must NOT fold these into the `bindings` list; the encoder must NOT
/// accept a `role=...` pair whose role collides with one of them.
const RESERVED_KEYS: &[&str] = &["extends", "primitive", "min_agents", "role_overrides"];

/// Build-time: if `section_path == "teams"`, fold every top-level scalar
/// string key on the entry table that is NOT a reserved key into a sorted
/// `bindings = ["role=agent", ...]` array so the schema renderer can pick it
/// up as a `StringList`. Returns `None` (no transform — caller falls through
/// to the unchanged `existing`) for every other section.
///
/// Non-string siblings (e.g. a `[role_overrides.<role>]` sub-table) are
/// preserved verbatim so the deferred `role_overrides` data survives the
/// round-trip even though the TUI doesn't expose it yet.
pub(crate) fn collapse_team_bindings_into_array(
    section_path: &str,
    existing: Option<&toml::Value>,
) -> Option<toml::Value> {
    if section_path != TEAMS_SECTION_PATH {
        return None;
    }
    let table = existing?.as_table()?;
    let mut out = toml::map::Map::new();
    let mut bindings: Vec<String> = Vec::new();
    for (k, v) in table {
        if RESERVED_KEYS.contains(&k.as_str()) {
            out.insert(k.clone(), v.clone());
            continue;
        }
        if let Some(agent) = v.as_str() {
            bindings.push(format!("{k}={agent}"));
        } else {
            out.insert(k.clone(), v.clone());
        }
    }
    bindings.sort();
    out.insert(
        "bindings".to_string(),
        toml::Value::Array(bindings.into_iter().map(toml::Value::String).collect()),
    );
    Some(toml::Value::Table(out))
}

/// Sync-time: split the `bindings = ["role=agent", ...]` array (the TUI
/// shape) into top-level scalar `role = "agent"` keys on `entry_table` (the
/// `#[serde(flatten)]` shape). Removes the array key after exploding.
///
/// Malformed pairs are dropped with a `tracing::warn!`. Drop conditions:
/// empty role, role that collides with a [`RESERVED_KEYS`] entry, item that
/// contains no `=` at all. `splitn(2, '=')` keeps any further `=` characters
/// in the agent value — an agent string `"value=with=equals"` survives.
pub(crate) fn explode_bindings_array_to_top_level(
    entry_table: &mut toml::map::Map<String, toml::Value>,
) {
    let Some(toml::Value::Array(items)) = entry_table.remove("bindings") else {
        return;
    };
    for item in items {
        let Some(pair) = item.as_str() else {
            continue;
        };
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let (Some(role), Some(agent)) = (parts.next(), parts.next()) else {
            tracing::warn!(pair = %pair, "teams bindings: dropping malformed entry (no `=`)");
            continue;
        };
        let role = role.trim();
        let agent = agent.trim();
        if role.is_empty() {
            tracing::warn!(pair = %pair, "teams bindings: dropping entry with empty role");
            continue;
        }
        if RESERVED_KEYS.contains(&role) {
            tracing::warn!(
                role = %role,
                "teams bindings: dropping entry whose role collides with a reserved schema key"
            );
            continue;
        }
        entry_table.insert(role.to_string(), toml::Value::String(agent.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_returns_none_for_non_teams_section() {
        let t = toml::Value::Table(toml::map::Map::new());
        assert!(collapse_team_bindings_into_array("agents", Some(&t)).is_none());
        assert!(collapse_team_bindings_into_array("modes", Some(&t)).is_none());
    }

    #[test]
    fn collapse_returns_none_when_existing_is_none() {
        assert!(collapse_team_bindings_into_array(TEAMS_SECTION_PATH, None).is_none());
    }

    #[test]
    fn collapse_folds_top_level_scalars_into_sorted_bindings_array() {
        let toml_str = r#"
            extends = "root"
            primitive = "pipeline"
            reviewer = "opencode"
            coder = "claude"
        "#;
        let v: toml::Value = toml::from_str(toml_str).unwrap();
        let out = collapse_team_bindings_into_array(TEAMS_SECTION_PATH, Some(&v)).unwrap();
        let tbl = out.as_table().unwrap();
        let arr = tbl.get("bindings").unwrap().as_array().unwrap();
        let items: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(items, vec!["coder=claude", "reviewer=opencode"]);
        assert_eq!(tbl.get("extends").unwrap().as_str(), Some("root"));
        assert_eq!(tbl.get("primitive").unwrap().as_str(), Some("pipeline"));
        assert!(
            tbl.get("coder").is_none() && tbl.get("reviewer").is_none(),
            "scalar bindings must be moved into the array, not duplicated"
        );
    }

    #[test]
    fn collapse_preserves_non_string_siblings_verbatim() {
        let toml_str = r#"
            extends = "root"
            coder = "claude"

            [role_overrides.reviewer]
            agent = "opencode"
        "#;
        let v: toml::Value = toml::from_str(toml_str).unwrap();
        let out = collapse_team_bindings_into_array(TEAMS_SECTION_PATH, Some(&v)).unwrap();
        let tbl = out.as_table().unwrap();
        assert!(
            tbl.get("role_overrides").is_some(),
            "role_overrides sub-table must survive the collapse"
        );
        let arr = tbl.get("bindings").unwrap().as_array().unwrap();
        let items: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(items, vec!["coder=claude"]);
        assert!(
            !items.iter().any(|s| s.starts_with("role_overrides")),
            "role_overrides must NOT appear in the bindings list"
        );
    }

    #[test]
    fn explode_writes_top_level_keys_and_removes_bindings_array() {
        let mut tbl = toml::map::Map::new();
        tbl.insert("extends".into(), toml::Value::String("root".into()));
        tbl.insert(
            "bindings".into(),
            toml::Value::Array(vec![
                toml::Value::String("coder=claude".into()),
                toml::Value::String("reviewer=opencode".into()),
            ]),
        );
        explode_bindings_array_to_top_level(&mut tbl);
        assert!(tbl.get("bindings").is_none());
        assert_eq!(tbl.get("coder").unwrap().as_str(), Some("claude"));
        assert_eq!(tbl.get("reviewer").unwrap().as_str(), Some("opencode"));
        assert_eq!(tbl.get("extends").unwrap().as_str(), Some("root"));
    }

    #[test]
    fn explode_drops_malformed_pairs_silently() {
        let mut tbl = toml::map::Map::new();
        tbl.insert(
            "bindings".into(),
            toml::Value::Array(vec![
                toml::Value::String("no_equals_sign".into()),
                toml::Value::String("".into()),
                toml::Value::String("   ".into()),
                toml::Value::String("=agent_with_no_role".into()),
                toml::Value::String("ok=value".into()),
                toml::Value::String("extends=should_be_rejected".into()),
            ]),
        );
        explode_bindings_array_to_top_level(&mut tbl);
        assert_eq!(tbl.get("ok").and_then(|v| v.as_str()), Some("value"));
        assert!(tbl.get("no_equals_sign").is_none());
        assert!(
            tbl.get("extends").is_none(),
            "reserved key `extends` must be rejected as a binding role"
        );
    }

    #[test]
    fn explode_keeps_equals_in_agent_value() {
        let mut tbl = toml::map::Map::new();
        tbl.insert(
            "bindings".into(),
            toml::Value::Array(vec![toml::Value::String("role=value=with=equals".into())]),
        );
        explode_bindings_array_to_top_level(&mut tbl);
        assert_eq!(
            tbl.get("role").and_then(|v| v.as_str()),
            Some("value=with=equals"),
            "splitn(2, '=') keeps every '=' after the first inside the agent value"
        );
    }

    #[test]
    fn explode_last_write_wins_for_duplicate_roles() {
        let mut tbl = toml::map::Map::new();
        tbl.insert(
            "bindings".into(),
            toml::Value::Array(vec![
                toml::Value::String("coder=claude".into()),
                toml::Value::String("coder=opencode".into()),
            ]),
        );
        explode_bindings_array_to_top_level(&mut tbl);
        assert_eq!(
            tbl.get("coder").and_then(|v| v.as_str()),
            Some("opencode"),
            "documented behaviour: duplicate role entries — last write wins"
        );
    }

    #[test]
    fn empty_bindings_emits_no_keys() {
        let mut tbl = toml::map::Map::new();
        tbl.insert("bindings".into(), toml::Value::Array(vec![]));
        explode_bindings_array_to_top_level(&mut tbl);
        assert!(
            tbl.is_empty(),
            "empty bindings array must leave the entry table empty (no `bindings = ...` line)"
        );
    }
}
