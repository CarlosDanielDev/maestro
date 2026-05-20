//! Tests for the Agents tab (`SettingsTab::Agents`, index 7) and the
//! cross-entry validation gate in `save_config`. See issue #792 §6 and §5.10.

use super::*;
use crate::config::AgentConfig;

#[test]
fn settings_tab_all_includes_agents_at_index_seven() {
    assert_eq!(SettingsTab::ALL[7], SettingsTab::Agents);
    assert_eq!(SettingsTab::ALL[7].label(), "Agents");
}

#[test]
fn settings_tab_all_includes_modes_at_index_eight() {
    assert_eq!(SettingsTab::ALL[8], SettingsTab::Modes);
    assert_eq!(SettingsTab::ALL[8].label(), "Modes");
}

#[test]
fn agents_tab_has_a_dynamic_map_widget() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let fields = &screen.fields_per_tab[7];
    assert!(
        !fields.is_empty(),
        "Agents tab (index 7) must have at least one field"
    );
    assert!(
        matches!(fields[0].widget, WidgetKind::DynamicMap(_)),
        "Agents tab's first field must be a DynamicMap widget, got {:?}",
        fields[0].widget.label()
    );
}

#[test]
fn modes_tab_has_a_dynamic_map_widget() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let fields = &screen.fields_per_tab[8];
    assert!(
        !fields.is_empty(),
        "Modes tab (index 8) must have at least one field"
    );
    assert!(
        matches!(fields[0].widget, WidgetKind::DynamicMap(_)),
        "Modes tab's first field must be a DynamicMap widget"
    );
}

#[test]
fn agents_default_pointing_at_unknown_entry_blocks_save_with_banner() {
    let (mut screen, _f) = screen_with_config_path();
    // Configure agents.default to point at a key that does not exist in entries.
    screen.config.agents.default = "qwen-fast".to_string();
    screen.config.agents.entries.clear();

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
        flash.contains("agents.default"),
        "save banner must mention `agents.default`, got: {flash:?}"
    );
}

#[test]
fn agents_save_preserves_comments_and_writes_section() {
    use std::io::Write;
    const COMMENTED: &str = "\
# This guardrail comment must survive saves
[project]
repo = \"owner/repo\"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
";
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(f, "{COMMENTED}").unwrap();
    let config = Config::load(f.path()).unwrap();
    let mut screen =
        SettingsScreen::new(config, make_flags()).with_config_path(f.path().to_path_buf());

    // Add the qwen-fast agent through the config layer (mirrors what the
    // DynamicMap Add modal does after the user types `qwen-fast` + Enter).
    let mut agent = AgentConfig::builtin_claude("qwen3", "default", Vec::new());
    agent.kind = crate::config::AgentKind::Qwen;
    agent.command = Some("qwen".to_string());
    agent.enabled = true;
    screen
        .config
        .agents
        .entries
        .insert("qwen-fast".to_string(), agent);
    screen.config.agents.default = "qwen-fast".to_string();

    let ctrl_s = Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    screen.handle_input(&ctrl_s, InputMode::Normal);

    let saved = std::fs::read_to_string(f.path()).unwrap();
    assert!(
        saved.contains("[agents.qwen-fast]"),
        "saved file must contain [agents.qwen-fast], got:\n{saved}"
    );
    assert!(
        saved.contains("# This guardrail comment must survive saves"),
        "comment must survive saves, got:\n{saved}"
    );
}

#[test]
fn agents_default_referencing_existing_entry_passes_save() {
    let (mut screen, _f) = screen_with_config_path();
    let mut agent = AgentConfig::builtin_claude("opus", "default", Vec::new());
    agent.enabled = true;
    screen
        .config
        .agents
        .entries
        .insert("claude".to_string(), agent);
    screen.config.agents.default = "claude".to_string();

    let ctrl_s = Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    screen.handle_input(&ctrl_s, InputMode::Normal);

    assert!(
        screen.save_error_flash.is_none(),
        "save banner must be empty for a valid agents.default, got {:?}",
        screen.save_error_flash
    );
}
