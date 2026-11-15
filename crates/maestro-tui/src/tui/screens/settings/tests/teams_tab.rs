//! Tests for the Teams tab (`SettingsTab::Teams`, index 9) and the
//! cross-entry `extends` validator surfaced via the Save banner.
//!
//! Issue #803 — wire `[teams.<id>]` through the schema renderer + the
//! StringList ↔ flattened-keys round-trip adapter.

use super::*;
use crate::orchestration::team::TeamConfig;
use crate::orchestration::types::Primitive;
use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::screens::settings::schema_tab::sync::sync_to_config;

const TEAMS_TAB_INDEX: usize = 9;

fn teams_table() -> &'static crate::config::schema::TableSchema {
    crate::config::schema::schema_for_config()
        .iter()
        .find(|t| t.name == "teams")
        .expect("teams schema must be registered")
}

#[test]
fn settings_tab_all_includes_teams_at_index_nine() {
    assert_eq!(SettingsTab::ALL[TEAMS_TAB_INDEX], SettingsTab::Teams);
    assert_eq!(SettingsTab::ALL[TEAMS_TAB_INDEX].label(), "Teams");
}

#[test]
fn teams_tab_has_a_dynamic_map_widget() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let fields = &screen.fields_per_tab[TEAMS_TAB_INDEX];
    assert_eq!(
        fields.len(),
        1,
        "Teams tab must have exactly the FlattenedMap entries widget"
    );
    assert!(
        matches!(fields[0].widget, WidgetKind::DynamicMap(_)),
        "Teams tab's first field must be a DynamicMap widget"
    );
}

#[test]
fn teams_tab_decoder_collapses_bindings_into_list_editor() {
    let toml_str = "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n\
[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
[github]\n[notifications]\n\
[teams.worker-pool]\nprimitive = \"pipeline\"\nimplementer = \"claude\"\nreviewer = \"opencode\"\n";
    let config: Config = toml::from_str(toml_str).expect("toml parses");
    let fields = from_schema(teams_table(), &config);
    let WidgetKind::DynamicMap(ref dm) = fields[0].widget else {
        panic!("teams tab must render as DynamicMap");
    };
    let entry = dm.entries().first().expect("one team entry");
    assert_eq!(entry.id, "worker-pool");
    // bindings is field index 3 (extends/primitive/min_agents/bindings).
    let WidgetKind::ListEditor(le) = &entry.fields[3].widget else {
        panic!(
            "bindings must render as ListEditor, got {:?}",
            entry.fields[3].widget.label()
        );
    };
    let mut items = le.items.clone();
    items.sort();
    assert_eq!(items, vec!["implementer=claude", "reviewer=opencode"]);
}

#[test]
fn teams_tab_encoder_writes_top_level_scalar_bindings() -> anyhow::Result<()> {
    let mut config: Config = toml::from_str(MINIMAL_SETTINGS_TOML).unwrap();
    let mut bindings = std::collections::HashMap::new();
    bindings.insert(
        "coder".to_string(),
        toml::Value::String("claude".to_string()),
    );
    config.teams.insert(
        "worker-pool".to_string(),
        TeamConfig {
            extends: String::new(),
            primitive: Some(Primitive::Pipeline),
            min_agents: None,
            bindings,
            role_overrides: std::collections::HashMap::new(),
        },
    );

    let fields = from_schema(teams_table(), &config);
    let mut config2 = config.clone();
    sync_to_config(teams_table(), &fields, &mut config2)?;

    let team = config2
        .teams
        .get("worker-pool")
        .expect("worker-pool must survive sync");
    assert_eq!(
        team.bindings.get("coder").and_then(|v| v.as_str()),
        Some("claude"),
        "encoder must restore `coder = \"claude\"` as a top-level binding key"
    );
    Ok(())
}

#[test]
fn teams_tab_reload_after_save_re_collapses_bindings() -> anyhow::Result<()> {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(f, "{}", MINIMAL_SETTINGS_TOML).unwrap();
    let mut config = Config::load(f.path())?;

    let mut bindings = std::collections::HashMap::new();
    bindings.insert(
        "implementer".to_string(),
        toml::Value::String("claude".to_string()),
    );
    config.teams.insert(
        "alpha".to_string(),
        TeamConfig {
            extends: String::new(),
            primitive: Some(Primitive::Pipeline),
            min_agents: None,
            bindings,
            role_overrides: std::collections::HashMap::new(),
        },
    );
    config.save(f.path())?;

    let reloaded = Config::load(f.path())?;
    let fields = from_schema(teams_table(), &reloaded);
    let WidgetKind::DynamicMap(ref dm) = fields[0].widget else {
        panic!("expected DynamicMap");
    };
    let entry = dm.entries().first().expect("one team after reload");
    assert_eq!(entry.id, "alpha");
    let WidgetKind::ListEditor(le) = &entry.fields[3].widget else {
        panic!("bindings field must be ListEditor");
    };
    assert!(
        le.items.contains(&"implementer=claude".to_string()),
        "bindings must survive the full disk round-trip — got {:?}",
        le.items
    );
    Ok(())
}

#[test]
fn pressing_a_on_bindings_field_adds_list_item_not_team() {
    // Regression: outer DynamicMap used to intercept `a` for the
    // Add-Team-Entry modal even when focus was on a child ListEditor
    // field, making it impossible to add a `bindings` entry. Fix
    // delegates ListEditor-owned chords (`a`, `d`, Enter) to the inner
    // widget when focus is on a ListEditor entry-field. See #803.
    use crate::tui::screens::settings::schema_tab::widgets::dynamic_map::{
        DynamicMapWidget, MapFocus,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    let mut existing = toml::map::Map::new();
    let mut entry = toml::map::Map::new();
    entry.insert("extends".into(), toml::Value::String("".into()));
    entry.insert("primitive".into(), toml::Value::String("pipeline".into()));
    existing.insert("worker-pool".into(), toml::Value::Table(entry));
    let existing_val = toml::Value::Table(existing);

    let mut widget = DynamicMapWidget::new(
        "entries",
        "teams",
        crate::config::schema::dynamic::TEAMS_ENTRY_FIELDS,
        Some(&existing_val),
    );
    // Move focus down to the bindings field (index 3) on the existing entry.
    widget.handle_input(key(KeyCode::Tab)); // SubtabStrip -> EntryField(0) extends
    widget.handle_input(key(KeyCode::Tab)); // -> EntryField(1) primitive
    widget.handle_input(key(KeyCode::Tab)); // -> EntryField(2) min_agents
    widget.handle_input(key(KeyCode::Tab)); // -> EntryField(3) bindings
    assert_eq!(*widget.focus(), MapFocus::EntryField(3));

    // Pressing `a` while focused on the bindings ListEditor must enter
    // its insert mode (RequestInsertMode), NOT open the Add-Team modal.
    widget.handle_input(key(KeyCode::Char('a')));
    assert_eq!(
        *widget.focus(),
        MapFocus::EntryField(3),
        "focus must stay on the bindings field, not jump to AddModal"
    );
    assert!(
        widget.needs_insert_mode(),
        "ListEditor must be in insert mode after `a`"
    );

    // Type a binding and commit with Enter.
    for c in "coder=claude".chars() {
        widget.handle_input(key(KeyCode::Char(c)));
    }
    widget.handle_input(key(KeyCode::Enter));

    // Inspect the entry; the bindings ListEditor must have the new item.
    let entry = widget
        .entries()
        .iter()
        .find(|e| e.id == "worker-pool")
        .expect("worker-pool present");
    let WidgetKind::ListEditor(ref le) = entry.fields[3].widget else {
        panic!("bindings field must be ListEditor");
    };
    assert!(
        le.items.contains(&"coder=claude".to_string()),
        "bindings list must contain `coder=claude`, got {:?}",
        le.items
    );
}

#[test]
fn desired_height_grows_for_bindings_listeditor_input() {
    // Regression for the "list hidden" bug: per-entry rows used to be
    // forced to a single line each via Constraint::Length(1), clipping
    // the `[a] Add  [d] Delete` empty-state hint and the input prompt
    // on the `bindings` ListEditor. After the fix, the widget reports
    // a desired_height that grows when a ListEditor row is focused or
    // editing so the surrounding settings draw can give it the rows it
    // needs.
    use crate::tui::screens::settings::schema_tab::widgets::dynamic_map::{
        DynamicMapWidget, MapFocus,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    let mut existing = toml::map::Map::new();
    let mut entry = toml::map::Map::new();
    entry.insert("primitive".into(), toml::Value::String("pipeline".into()));
    existing.insert("alpha".into(), toml::Value::Table(entry));
    let val = toml::Value::Table(existing);

    let mut widget = DynamicMapWidget::new(
        "entries",
        "teams",
        crate::config::schema::dynamic::TEAMS_ENTRY_FIELDS,
        Some(&val),
    );
    // Baseline (focus on SubtabStrip): all 4 fields at 1 line each.
    let base = widget.desired_height();

    // Focus bindings (idx 3, empty ListEditor) — should grow by +1 for
    // the `[a] Add  [d] Delete` hint that now has somewhere to render.
    for _ in 0..4 {
        widget.handle_input(key(KeyCode::Tab));
    }
    assert_eq!(*widget.focus(), MapFocus::EntryField(3));
    let focused = widget.desired_height();
    assert!(
        focused > base,
        "focused bindings ListEditor must grow desired_height: base={base} focused={focused}"
    );

    // Enter edit mode on the bindings ListEditor — should grow further to
    // accommodate the input prompt row.
    widget.handle_input(key(KeyCode::Char('a')));
    assert!(widget.needs_insert_mode(), "ListEditor in edit mode");
    let editing = widget.desired_height();
    assert!(
        editing >= focused,
        "editing bindings ListEditor must not shrink desired_height: focused={focused} editing={editing}"
    );
}

#[test]
fn edit_hint_on_bindings_field_shows_listeditor_chord_not_team_chord() {
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

    let mut existing = toml::map::Map::new();
    let mut entry = toml::map::Map::new();
    entry.insert("primitive".into(), toml::Value::String("pipeline".into()));
    existing.insert("alpha".into(), toml::Value::Table(entry));
    let val = toml::Value::Table(existing);

    let mut widget = DynamicMapWidget::new(
        "entries",
        "teams",
        crate::config::schema::dynamic::TEAMS_ENTRY_FIELDS,
        Some(&val),
    );

    // SubtabStrip focus: outer Add/Del hint.
    let hint = widget.edit_hint();
    assert!(
        hint.iter().any(|(k, _)| *k == "a/d"),
        "SubtabStrip focus must surface the outer a/d Add/Del chord"
    );

    // Move focus onto the bindings ListEditor (idx 3).
    for _ in 0..4 {
        widget.handle_input(key(KeyCode::Tab));
    }
    let hint = widget.edit_hint();
    assert!(
        hint.iter().any(|(k, _)| *k == "Enter"),
        "ListEditor-focused hint must surface the Enter chord, got {hint:?}"
    );
    assert!(
        !hint.iter().any(|(k, _)| *k == "a/d"),
        "ListEditor-focused hint must NOT show the outer a/d chord, got {hint:?}"
    );
}

#[test]
fn pressing_a_on_role_overrides_field_opens_nested_add_modal_not_outer() {
    // #901 — pressing `a` while focus is on the nested role_overrides
    // DynamicMap field must open the INNER add-role modal, NOT the
    // outer "Add team entry" modal. Mirrors the pattern of
    // `pressing_a_on_bindings_field_adds_list_item_not_team` (#803).
    use crate::tui::screens::settings::schema_tab::widgets::dynamic_map::{
        DynamicMapWidget, MapFocus,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    let mut existing = toml::map::Map::new();
    let mut entry = toml::map::Map::new();
    entry.insert("extends".into(), toml::Value::String("".into()));
    entry.insert("primitive".into(), toml::Value::String("pipeline".into()));
    existing.insert("worker-pool".into(), toml::Value::Table(entry));
    let existing_val = toml::Value::Table(existing);

    let mut widget = DynamicMapWidget::new(
        "entries",
        "teams",
        crate::config::schema::dynamic::TEAMS_ENTRY_FIELDS,
        Some(&existing_val),
    );
    // Walk SubtabStrip -> EntryField(0..=4) reaching role_overrides (idx 4).
    for _ in 0..5 {
        widget.handle_input(key(KeyCode::Tab));
    }
    assert_eq!(*widget.focus(), MapFocus::EntryField(4));

    widget.handle_input(key(KeyCode::Char('a')));

    // Outer modal stays closed.
    assert_eq!(
        *widget.focus(),
        MapFocus::EntryField(4),
        "outer focus must stay on role_overrides field, not jump to AddModal"
    );

    let entry = widget
        .entries()
        .iter()
        .find(|e| e.id == "worker-pool")
        .expect("worker-pool present");
    let WidgetKind::DynamicMap(ref inner) = entry.fields[4].widget else {
        panic!(
            "role_overrides field must be DynamicMap, got label {:?}",
            entry.fields[4].widget.label()
        );
    };
    assert!(
        matches!(inner.focus(), MapFocus::AddModal),
        "inner DynamicMap must have opened its add-role modal in response to `a`"
    );
}

#[test]
fn teams_with_unknown_extends_blocks_save_with_banner() {
    let (mut screen, _f) = screen_with_config_path();
    screen.config.teams.insert(
        "orphan".to_string(),
        TeamConfig {
            extends: "ghost-parent".to_string(),
            primitive: Some(Primitive::Pipeline),
            min_agents: None,
            bindings: std::collections::HashMap::new(),
            role_overrides: std::collections::HashMap::new(),
        },
    );

    let ctrl_s = Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    screen.handle_input(&ctrl_s, InputMode::Normal);

    let flash = screen
        .save_error_flash
        .as_ref()
        .map(|(msg, _)| msg.as_str())
        .unwrap_or("");
    assert!(
        flash.contains("teams.orphan.extends"),
        "save banner must surface the teams path, got: {flash:?}"
    );
    assert!(
        flash.contains("ghost-parent"),
        "save banner must name the unknown parent, got: {flash:?}"
    );
}

#[test]
fn build_role_override_lookup_maps_warnings_to_structured_paths() {
    // #909 — pins the contract that `SettingsScreen::build_role_override_lookup`
    // produces a HashMap keyed by `RoleOverrideWarning::structured_path()`
    // and that each value is a `Warning`-severity ValidationFeedback whose
    // message names the offending value. The render-path plumbing in
    // draw_with_warnings depends on this exact key shape.
    use crate::orchestration::team_role_overrides::{RoleOverrideField, RoleOverrideWarning};
    use crate::tui::screens::settings::validation::ValidationSeverity;

    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.set_role_override_warnings_for_test(vec![
        RoleOverrideWarning {
            team_id: "worker-pool".to_string(),
            role_id: "reviewer".to_string(),
            field: RoleOverrideField::Agent,
            value: "nonexistent-agent".to_string(),
        },
        RoleOverrideWarning {
            team_id: "worker-pool".to_string(),
            role_id: "reviewer".to_string(),
            field: RoleOverrideField::Mode,
            value: "ghost-mode".to_string(),
        },
    ]);

    let lookup = screen.build_role_override_lookup();
    assert_eq!(lookup.len(), 2, "one entry per warning");

    let agent_fb = lookup
        .get("teams.worker-pool.role_overrides.reviewer.agent")
        .expect("agent key must exist in lookup");
    assert_eq!(agent_fb.severity, ValidationSeverity::Warning);
    assert!(
        agent_fb.message.contains("nonexistent-agent"),
        "agent message must reference the bad value, got: {}",
        agent_fb.message
    );

    let mode_fb = lookup
        .get("teams.worker-pool.role_overrides.reviewer.mode")
        .expect("mode key must exist in lookup");
    assert!(
        mode_fb.message.contains("ghost-mode"),
        "mode message must reference the bad value, got: {}",
        mode_fb.message
    );
    // #912 — empty `[agents.*]` / `[modes.*]` in MINIMAL_SETTINGS_TOML
    // means the valid-list tail collapses to `no … configured`.
    assert!(
        agent_fb.message.ends_with("no agents configured"),
        "empty agents set must produce `no agents configured` tail, got: {}",
        agent_fb.message
    );
    assert!(
        mode_fb.message.ends_with("no modes configured"),
        "empty modes set must produce `no modes configured` tail, got: {}",
        mode_fb.message
    );
}

#[test]
fn build_role_override_lookup_lists_configured_ids_in_warning_tail() {
    // #912 — when `[agents.*]` and `[modes.*]` are configured, the
    // inline warning message lists those ids so the user can pick a
    // valid value without leaving Settings.
    use crate::orchestration::team_role_overrides::{RoleOverrideField, RoleOverrideWarning};

    let toml_str = "\
[project]
repo = \"owner/repo\"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]

[agents.claude]
kind = \"claude\"
command = \"claude\"

[agents.opencode]
kind = \"opencode\"
command = \"opencode\"

[modes.review-strict]
prompt_addendum = \"Be terse\"
";
    let mut f = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write;
    write!(f, "{toml_str}").unwrap();
    let config = Config::load(f.path()).unwrap();
    let mut screen = SettingsScreen::new(config, make_flags());

    screen.set_role_override_warnings_for_test(vec![
        RoleOverrideWarning {
            team_id: "t".to_string(),
            role_id: "r".to_string(),
            field: RoleOverrideField::Agent,
            value: "ghost".to_string(),
        },
        RoleOverrideWarning {
            team_id: "t".to_string(),
            role_id: "r".to_string(),
            field: RoleOverrideField::Mode,
            value: "ghost-mode".to_string(),
        },
    ]);

    let lookup = screen.build_role_override_lookup();
    let agent_fb = lookup
        .get("teams.t.role_overrides.r.agent")
        .expect("agent key");
    assert_eq!(
        agent_fb.message, "unknown agent `ghost` — valid: claude, opencode",
        "agent warning must enumerate configured agent ids",
    );
    let mode_fb = lookup
        .get("teams.t.role_overrides.r.mode")
        .expect("mode key");
    assert_eq!(
        mode_fb.message, "unknown mode `ghost-mode` — valid: review-strict",
        "mode warning must enumerate configured mode ids",
    );
}

#[test]
fn build_role_override_lookup_strips_terminal_escape_sequences() {
    // #909 security finding (low): role-override values come from
    // on-disk TOML and pass through ratatui Paragraphs that emit raw
    // bytes. ESC and other C0/C1 controls must be neutralised before
    // they reach the back-buffer — same `sanitize_for_terminal` path
    // the Save-banner uses on the structured path.
    use crate::orchestration::team_role_overrides::{RoleOverrideField, RoleOverrideWarning};

    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.set_role_override_warnings_for_test(vec![RoleOverrideWarning {
        team_id: "worker-pool".to_string(),
        role_id: "reviewer".to_string(),
        field: RoleOverrideField::Agent,
        value: "\u{001b}[31mEVIL\u{001b}[0m".to_string(),
    }]);

    let lookup = screen.build_role_override_lookup();
    let fb = lookup
        .get("teams.worker-pool.role_overrides.reviewer.agent")
        .expect("agent key must exist");
    assert!(
        !fb.message.contains('\u{001b}'),
        "ESC must be stripped, got bytes: {:?}",
        fb.message.as_bytes()
    );
}
