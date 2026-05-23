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
