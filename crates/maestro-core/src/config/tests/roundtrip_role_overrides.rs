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
fn tui_driven_add_role_serializes_with_all_five_subfields() {
    // #901 — drive the outer Teams DynamicMap via handle_input to:
    //   1. focus the role_overrides field (idx 4) of an existing team
    //   2. press `a` (chord delegated to inner DynamicMap)
    //   3. submit an id "architect"
    //   4. serialize the outer widget
    //   5. assert the new role sub-table is present with default values
    //      for all five fields (empty strings — they survive the round-trip
    //      via the typed Option<String> re-serialization in Config -> toml)
    use crate::tui::screens::settings::schema_tab::widgets::dynamic_map::DynamicMapWidget;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }
    fn typed(w: &mut DynamicMapWidget, s: &str) {
        for c in s.chars() {
            w.handle_input(key(KeyCode::Char(c)));
        }
    }

    let mut team = toml::map::Map::new();
    team.insert("extends".into(), toml::Value::String("".into()));
    team.insert("primitive".into(), toml::Value::String("pipeline".into()));
    let mut outer = toml::map::Map::new();
    outer.insert("worker-pool".into(), toml::Value::Table(team));
    let val = toml::Value::Table(outer);

    let mut widget = DynamicMapWidget::new(
        "entries",
        "teams",
        crate::config::schema::dynamic::TEAMS_ENTRY_FIELDS,
        Some(&val),
    );
    // SubtabStrip -> EntryField(0..=4) = role_overrides
    for _ in 0..5 {
        widget.handle_input(key(KeyCode::Tab));
    }
    // `a` reaches the inner DynamicMap; inner.focus = AddModal.
    widget.handle_input(key(KeyCode::Char('a')));
    // Type the role id and submit; inner inserts the new role entry.
    typed(&mut widget, "architect");
    widget.handle_input(key(KeyCode::Enter));

    let serialized = widget.serialize_to_toml();
    let teams_t = serialized.as_table().expect("table");
    let wp = teams_t
        .get("worker-pool")
        .and_then(|v| v.as_table())
        .expect("worker-pool entry must survive serialization");
    let ro = wp
        .get("role_overrides")
        .and_then(|v| v.as_table())
        .expect("role_overrides sub-table must be emitted after add");
    let architect = ro
        .get("architect")
        .and_then(|v| v.as_table())
        .expect("architect role sub-table must be present");
    for k in [
        "agent",
        "mode",
        "model_override",
        "prompt_addendum",
        "fallback_agent",
    ] {
        assert!(
            architect.contains_key(k),
            "architect role must register all 5 sub-fields after TUI-driven add; missing {k}"
        );
    }
}

#[test]
fn tui_driven_edit_prompt_addendum_round_trips() {
    // #901 — start from a team that already has a `reviewer` role,
    // drive focus into the nested prompt_addendum field, type a new
    // value, then serialize the outer widget and assert the value
    // survived the writeback.
    use crate::tui::screens::settings::schema_tab::widgets::dynamic_map::DynamicMapWidget;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }
    fn typed(w: &mut DynamicMapWidget, s: &str) {
        for c in s.chars() {
            w.handle_input(key(KeyCode::Char(c)));
        }
    }

    // Build outer DynamicMap from the existing on-disk shape produced by
    // `worker_pool_team_with_overrides()` to keep parity with the
    // existing Config-level round-trip tests.
    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "teams_add.toml.before");
    cfg.teams
        .insert("worker-pool".to_string(), worker_pool_team_with_overrides());
    let saved = cfg
        .save_into_str(&before)
        .expect("baseline save must succeed");

    // Re-parse and rebuild the outer DynamicMap via the schema renderer,
    // exactly as the TUI does on screen entry.
    let tmp = temp_file_with(&saved);
    let cfg2 = Config::load(tmp.path()).expect("baseline reload");
    let teams_table = schema_for_config()
        .iter()
        .find(|t| t.name == "teams")
        .expect("teams schema");
    let fields = crate::tui::screens::settings::schema_tab::build::from_schema(teams_table, &cfg2);
    let mut outer_widget = match fields.into_iter().next().expect("teams widget").widget {
        crate::tui::widgets::WidgetKind::DynamicMap(dm) => dm,
        other => panic!(
            "teams tab must render as DynamicMap, got {:?}",
            other.label()
        ),
    };

    // Walk to role_overrides field (idx 4) on the worker-pool entry.
    for _ in 0..5 {
        outer_widget.handle_input(key(KeyCode::Tab));
    }
    // Inner DynamicMap is focused on its SubtabStrip with the `reviewer`
    // role as active. Drive `Down` keys to walk inner SubtabStrip -> agent
    // -> mode -> model_override -> prompt_addendum (index 3 inside ROLE_OVERRIDE_FIELDS).
    //
    // The outer's KeyCode::Down handler does NOT currently delegate to a
    // nested DynamicMap, so press the keys directly against the inner
    // widget. The chord routing for `a/d/u` IS exercised end-to-end above.
    let entry_idx = outer_widget
        .entries()
        .iter()
        .position(|e| e.id == "worker-pool")
        .expect("worker-pool present");
    // Drive the inner DynamicMap by walking down through its fields,
    // entering insert mode on prompt_addendum, replacing the value, then
    // committing — all via the OUTER widget's handle_input so the chord
    // delegation paths are real.
    //
    // Each `Down` press on the outer is routed to the inner via the
    // `_ =>` fall-through arm of the outer's handle_input (since
    // focus is on EntryField(4), the outer delegates the inner widget's
    // handle_input call which advances the inner focus). This mirrors
    // production behavior.
    for _ in 0..4 {
        outer_widget.handle_input(key(KeyCode::Down));
    }
    // Now press Enter to enter insert-mode on the focused inner field
    // (the inner's `_ =>` arm forwards Enter to the inner widget which
    // is a TextInput → toggles editing).
    outer_widget.handle_input(key(KeyCode::Enter));
    // Backspace to clear the old "Be terse" prompt then type the new value.
    for _ in 0.."Be terse".len() {
        outer_widget.handle_input(key(KeyCode::Backspace));
    }
    typed(&mut outer_widget, "Focus on correctness");
    outer_widget.handle_input(key(KeyCode::Enter));

    let serialized = outer_widget.serialize_to_toml();
    let teams_t = serialized.as_table().expect("table");
    let wp = teams_t
        .get("worker-pool")
        .and_then(|v| v.as_table())
        .expect("worker-pool present");
    let ro = wp
        .get("role_overrides")
        .and_then(|v| v.as_table())
        .expect("role_overrides present");
    let reviewer = ro
        .get("reviewer")
        .and_then(|v| v.as_table())
        .expect("reviewer role present");
    // The edited prompt_addendum survives. Sibling fields preserved.
    let _ = entry_idx; // silence unused (used to verify worker-pool was located before Tab navigation)
    assert_eq!(
        reviewer.get("prompt_addendum").and_then(|v| v.as_str()),
        Some("Focus on correctness"),
        "edited prompt_addendum must survive TUI writeback; full reviewer = {reviewer:?}"
    );
    assert_eq!(
        reviewer.get("agent").and_then(|v| v.as_str()),
        Some("opencode"),
        "agent must survive untouched: {reviewer:?}"
    );
    assert_eq!(
        reviewer.get("mode").and_then(|v| v.as_str()),
        Some("review-strict"),
        "mode must survive untouched: {reviewer:?}"
    );
    assert_eq!(
        reviewer.get("fallback_agent").and_then(|v| v.as_str()),
        Some("claude"),
        "fallback_agent must survive untouched: {reviewer:?}"
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
