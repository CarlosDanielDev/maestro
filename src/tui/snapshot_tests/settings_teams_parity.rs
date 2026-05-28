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
    assert!(
        matches!(role_overrides_field.widget, WidgetKind::DynamicMap(_)),
        "role_overrides must now be a nested DynamicMap (#901), got label {:?}",
        role_overrides_field.widget.label()
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

#[test]
fn nested_role_overrides_widget_renders_breadcrumb_when_focused() {
    // #908 — when the inner role_overrides DynamicMap is focused (the
    // outer DynamicMap has the role_overrides slot focused), its
    // header row carries the breadcrumb `teams.<id> → role_overrides →
    // <role>` instead of the plain label.
    let screen = SettingsScreen::new(
        test_config_team_with_role_overrides(),
        FeatureFlags::default(),
    );
    let fields = &screen.fields_per_tab[TEAMS_TAB_INDEX];
    let WidgetKind::DynamicMap(ref outer) = fields[0].widget else {
        panic!("teams tab must render as DynamicMap");
    };
    let role_overrides_field = outer
        .entries()
        .first()
        .and_then(|e| {
            e.fields
                .iter()
                .find(|f| f.widget.label().ends_with(".role_overrides"))
        })
        .expect("entry must expose a role_overrides field");
    let WidgetKind::DynamicMap(ref inner) = role_overrides_field.widget else {
        panic!("role_overrides must be a nested DynamicMap");
    };
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 12)).expect("backend");
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            inner.draw(f, f.area(), &theme, true);
        })
        .expect("draw");
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(
        rendered.contains("teams.worker-pool"),
        "breadcrumb outer crumb missing in {rendered}"
    );
    assert!(
        rendered.contains("role_overrides"),
        "breadcrumb inner-field crumb missing in {rendered}"
    );
    assert!(
        rendered.contains("→"),
        "breadcrumb separator missing in {rendered}"
    );
}

#[test]
fn nested_role_overrides_widget_renders_plain_label_when_unfocused() {
    // #908 — when the inner role_overrides DynamicMap is NOT focused,
    // the header shows the plain `<label>:` so users on the outer
    // SubtabStrip see no premature breadcrumb chrome.
    let screen = SettingsScreen::new(
        test_config_team_with_role_overrides(),
        FeatureFlags::default(),
    );
    let fields = &screen.fields_per_tab[TEAMS_TAB_INDEX];
    let WidgetKind::DynamicMap(ref outer) = fields[0].widget else {
        panic!("teams tab must render as DynamicMap");
    };
    let role_overrides_field = outer
        .entries()
        .first()
        .and_then(|e| {
            e.fields
                .iter()
                .find(|f| f.widget.label().ends_with(".role_overrides"))
        })
        .expect("entry must expose a role_overrides field");
    let WidgetKind::DynamicMap(ref inner) = role_overrides_field.widget else {
        panic!("role_overrides must be a nested DynamicMap");
    };
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 12)).expect("backend");
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            inner.draw(f, f.area(), &theme, false);
        })
        .expect("draw");
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(
        !rendered.contains("→"),
        "unfocused inner widget must NOT render a breadcrumb, got {rendered}"
    );
    assert!(
        rendered.contains("teams.worker-pool.role_overrides:"),
        "unfocused inner widget must render the plain label, got {rendered}"
    );
}

#[test]
fn save_banner_lists_structured_path_for_unknown_role_override_agent() {
    // #908 — saving with a role_overrides.<role>.agent that doesn't
    // resolve in [agents.<id>] records a soft warning. The Save banner
    // title lists the structured path on the next render.
    let mut config = test_config_team_with_role_overrides();
    config
        .teams
        .get_mut("worker-pool")
        .unwrap()
        .role_overrides
        .get_mut("reviewer")
        .unwrap()
        .agent = Some("nonexistent-agent".to_string());
    let mut screen = SettingsScreen::new(config, FeatureFlags::default());
    // Inject known id sets so only the deliberately-broken `agent`
    // override flags (the fixture's mode/fallback_agent values resolve).
    let mut known_agents = std::collections::BTreeSet::new();
    known_agents.insert("opencode".to_string());
    known_agents.insert("claude".to_string());
    let mut known_modes = std::collections::BTreeSet::new();
    known_modes.insert("review-strict".to_string());
    let warnings = crate::orchestration::team_role_overrides::validate_role_overrides(
        &screen.config.teams,
        &known_agents,
        &known_modes,
    );
    assert_eq!(warnings.len(), 1, "must flag exactly the unknown agent");
    assert_eq!(
        warnings[0].structured_path(),
        "teams.worker-pool.role_overrides.reviewer.agent",
    );
    // Confirm the banner-summary helper formats the path correctly.
    screen.set_role_override_warnings_for_test(warnings);
    let summary = screen
        .role_override_warnings_summary()
        .expect("warnings must produce a summary");
    assert_eq!(summary, "teams.worker-pool.role_overrides.reviewer.agent");
}
