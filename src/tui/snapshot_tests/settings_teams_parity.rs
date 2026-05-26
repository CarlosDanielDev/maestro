//! Parity tests for the Teams tab schema-driven wiring (issue #803).
//!
//! Teams sits at index 9, between Modes (8) and Theme (10). Renders via the
//! same `FlattenedMap` → `DynamicMapWidget` path as Agents/Modes. The
//! StringList ↔ flattened-keys round-trip for `bindings` is covered in
//! `src/config/tests/roundtrip_overlay_dynamic.rs` and the
//! `teams_bindings` module's unit tests; this file pins the on-screen field
//! shape + per-entry rendering.

use insta::assert_snapshot;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::Config;
use crate::flags::store::FeatureFlags;
use crate::orchestration::team::RoleOverride;
use crate::orchestration::team::TeamConfig;
use crate::orchestration::types::Primitive;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::SettingsScreen;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

const TEAMS_TAB_INDEX: usize = 9;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
);

fn test_config_no_teams() -> Config {
    toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML must parse")
}

fn test_config_one_team() -> Config {
    let mut config = test_config_no_teams();
    let mut bindings = std::collections::HashMap::new();
    bindings.insert(
        "implementer".to_string(),
        toml::Value::String("claude".to_string()),
    );
    config.teams.insert(
        "worker-pool".to_string(),
        TeamConfig {
            extends: String::new(),
            primitive: Some(Primitive::Pipeline),
            min_agents: Some(vec!["claude".to_string()]),
            bindings,
            role_overrides: std::collections::HashMap::new(),
        },
    );
    config
}

fn test_config_team_with_role_overrides() -> Config {
    let mut config = test_config_no_teams();
    let mut bindings = std::collections::HashMap::new();
    bindings.insert(
        "implementer".to_string(),
        toml::Value::String("claude".to_string()),
    );
    let mut role_overrides = std::collections::HashMap::new();
    role_overrides.insert(
        "reviewer".to_string(),
        RoleOverride {
            agent: Some("opencode".to_string()),
            mode: Some("review-strict".to_string()),
            model_override: None,
            prompt_addendum: Some("Be terse".to_string()),
            fallback_agent: Some("claude".to_string()),
        },
    );
    config.teams.insert(
        "worker-pool".to_string(),
        TeamConfig {
            extends: String::new(),
            primitive: Some(Primitive::Pipeline),
            min_agents: Some(vec!["claude".to_string()]),
            bindings,
            role_overrides,
        },
    );
    config
}

fn render_tab(fields: &[SettingsField], width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("TestBackend must init");
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            let area = f.area();
            for (i, field) in fields.iter().enumerate() {
                let y = i as u16;
                if y >= area.height {
                    break;
                }
                let row = Rect {
                    x: area.x,
                    y: area.y + y,
                    width: area.width,
                    height: 1,
                };
                field.widget.draw(f, row, &theme, i == 0, None);
            }
        })
        .expect("draw must succeed");
    terminal.backend().buffer().clone()
}

#[test]
fn teams_tab_empty_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config_no_teams(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[TEAMS_TAB_INDEX];
    assert_eq!(
        fields.len(),
        1,
        "Teams tab must have exactly the FlattenedMap entries widget"
    );
    assert_eq!(fields[0].widget.label(), "entries");
    assert!(
        matches!(fields[0].widget, WidgetKind::DynamicMap(_)),
        "Teams tab field must be DynamicMap, got {:?}",
        fields[0].widget.label()
    );
}

#[test]
fn teams_tab_one_entry_exposes_bindings_list_editor() {
    let screen = SettingsScreen::new(test_config_one_team(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[TEAMS_TAB_INDEX];
    let WidgetKind::DynamicMap(ref dm) = fields[0].widget else {
        panic!("teams tab must render as DynamicMap");
    };
    let entry = dm.entries().first().expect("one entry must be present");
    assert_eq!(entry.id, "worker-pool");
    // bindings is field index 3 (extends/primitive/min_agents/bindings)
    let WidgetKind::ListEditor(le) = &entry.fields[3].widget else {
        panic!(
            "bindings must be ListEditor, got {:?}",
            entry.fields[3].widget.label()
        );
    };
    assert!(
        le.items.contains(&"implementer=claude".to_string()),
        "bindings ListEditor must contain 'implementer=claude', got {:?}",
        le.items
    );
}

#[test]
fn teams_tab_empty_renders_80x24() {
    let screen = SettingsScreen::new(test_config_no_teams(), FeatureFlags::default());
    let buf = render_tab(&screen.fields_per_tab[TEAMS_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn teams_tab_one_entry_renders_80x24() {
    let screen = SettingsScreen::new(test_config_one_team(), FeatureFlags::default());
    let buf = render_tab(&screen.fields_per_tab[TEAMS_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn teams_tab_one_entry_with_role_overrides_exposes_role_overrides_field() {
    let screen = SettingsScreen::new(
        test_config_team_with_role_overrides(),
        FeatureFlags::default(),
    );
    let fields = &screen.fields_per_tab[TEAMS_TAB_INDEX];
    let WidgetKind::DynamicMap(ref dm) = fields[0].widget else {
        panic!("teams tab must render as DynamicMap");
    };
    let entry = dm.entries().first().expect("one entry must be present");
    assert_eq!(
        entry.fields.len(),
        5,
        "entry must expose 5 schema fields after #872"
    );
    let role_overrides_field = entry
        .fields
        .iter()
        .find(|f| f.widget.label().ends_with(".role_overrides"))
        .expect("entry must expose a role_overrides field");
    assert_eq!(
        role_overrides_field.widget.label(),
        "teams.worker-pool.role_overrides",
    );
}

#[test]
fn teams_tab_one_entry_with_role_overrides_renders_80x24() {
    let screen = SettingsScreen::new(
        test_config_team_with_role_overrides(),
        FeatureFlags::default(),
    );
    let buf = render_tab(&screen.fields_per_tab[TEAMS_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}
