//! Round-trip tests for `[teams.<id>.role_overrides.<role>]` — issue #872.
//!
//! Split from `roundtrip_overlay_dynamic.rs` to keep both files under the
//! 400-line guardrail (`docs/RUST-GUARDRAILS.md` §7).

use super::roundtrip_overlay::{load_fixture, temp_file_with};
use super::*;
use crate::config::schema::{FieldKind, schema_for_config};
use crate::orchestration::team::{RoleOverride, TeamConfig};
use crate::orchestration::types::Primitive;
use std::collections::HashMap;

const FIXTURE_DIR: &str = "tests/fixtures/dynamic_config";

fn worker_pool_team_no_overrides() -> TeamConfig {
    let mut bindings: HashMap<String, toml::Value> = HashMap::new();
    bindings.insert(
        "implementer".to_string(),
        toml::Value::String("claude".to_string()),
    );
    TeamConfig {
        extends: String::new(),
        primitive: Some(Primitive::Pipeline),
        min_agents: Some(vec!["claude".to_string()]),
        bindings,
        role_overrides: HashMap::new(),
    }
}

fn worker_pool_team_with_overrides() -> TeamConfig {
    let mut bindings: HashMap<String, toml::Value> = HashMap::new();
    bindings.insert(
        "implementer".to_string(),
        toml::Value::String("claude".to_string()),
    );
    let mut role_overrides: HashMap<String, RoleOverride> = HashMap::new();
    role_overrides.insert(
        "reviewer".to_string(),
        RoleOverride {
            agent: Some("opencode".to_string()),
            mode: Some("review-strict".to_string()),
            model_override: Some("gpt-4o-mini".to_string()),
            prompt_addendum: Some("Be terse".to_string()),
            fallback_agent: Some("claude".to_string()),
        },
    );
    TeamConfig {
        extends: String::new(),
        primitive: Some(Primitive::Pipeline),
        min_agents: Some(vec!["claude".to_string()]),
        bindings,
        role_overrides,
    }
}

fn assert_role_overrides_schema_slot_registered() {
    let teams_table = schema_for_config()
        .iter()
        .find(|t| t.name == "teams")
        .expect("teams table must be registered");
    let FieldKind::FlattenedMap { entry_fields } = teams_table
        .fields
        .iter()
        .find(|f| matches!(f.kind, FieldKind::FlattenedMap { .. }))
        .expect("teams must expose a FlattenedMap field")
        .kind
    else {
        panic!("teams FlattenedMap variant required");
    };
    assert!(
        entry_fields.iter().any(|f| f.key == "role_overrides"),
        "TEAMS_ENTRY_FIELDS must register the role_overrides schema slot \
         before the round-trip path can be considered correct (#872)"
    );
}

#[test]
fn add_teams_entry_with_role_overrides_preserves_all_five_optional_fields() {
    assert_role_overrides_schema_slot_registered();

    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "teams_add.toml.before");
    cfg.teams
        .insert("worker-pool".to_string(), worker_pool_team_with_overrides());

    let after = cfg
        .save_into_str(&before)
        .expect("save_into_str must succeed");

    assert!(
        after.contains("[teams.worker-pool.role_overrides.reviewer]"),
        "role_overrides sub-table header must be emitted:\n{after}"
    );
    for line in [
        "agent = \"opencode\"",
        "mode = \"review-strict\"",
        "model_override = \"gpt-4o-mini\"",
        "prompt_addendum = \"Be terse\"",
        "fallback_agent = \"claude\"",
    ] {
        assert!(
            after.contains(line),
            "RoleOverride field line missing: {line:?}\n{after}"
        );
    }

    let tmp2 = temp_file_with(&after);
    let cfg2 = Config::load(tmp2.path()).expect("after must parse");
    let after2 = cfg2.save_into_str(&after).expect("re-save");
    assert_eq!(
        after, after2,
        "second save with no mutations must be byte-identical"
    );
}

#[test]
fn remove_role_override_from_team_drops_subtable_without_dropping_bindings() {
    assert_role_overrides_schema_slot_registered();

    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "teams_add.toml.before");
    cfg.teams
        .insert("worker-pool".to_string(), worker_pool_team_with_overrides());
    let pass1 = cfg
        .save_into_str(&before)
        .expect("first save with overrides");

    let tmp2 = temp_file_with(&pass1);
    let mut cfg2 = Config::load(tmp2.path()).expect("pass1 must parse");
    cfg2.teams
        .get_mut("worker-pool")
        .expect("worker-pool present")
        .role_overrides
        .clear();

    let pass2 = cfg2.save_into_str(&pass1).expect("second save");
    assert!(
        !pass2.contains("[teams.worker-pool.role_overrides"),
        "role_overrides sub-table must be gone after clear:\n{pass2}"
    );
    assert!(
        pass2.contains("implementer = \"claude\""),
        "bindings must survive the role_overrides removal:\n{pass2}"
    );
}

#[test]
fn empty_role_overrides_map_does_not_emit_subtable_header() {
    assert_role_overrides_schema_slot_registered();

    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "teams_add.toml.before");
    cfg.teams
        .insert("worker-pool".to_string(), worker_pool_team_no_overrides());

    let after = cfg
        .save_into_str(&before)
        .expect("save_into_str must succeed");

    assert!(
        !after.contains("role_overrides"),
        "empty role_overrides map must not emit any role_overrides token:\n{after}"
    );
}
