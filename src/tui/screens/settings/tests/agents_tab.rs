//! Tests for the Agents tab (`SettingsTab::Agents`, index 7) and the
//! cross-entry validation gate in `save_config`. See issue #792 §6 and §5.10.

use super::*;
use crate::config::AgentConfig;
use crate::tui::screens::settings::schema_tab::widgets::dynamic_map::MapFocus;

#[test]
fn settings_tab_all_includes_agents_at_index_seven() {
    assert_eq!(SettingsTab::ALL[7], SettingsTab::Agents);
    // Display label renamed from "Agents" to "Providers" — the TOML
    // section + Rust types stay AgentConfig/AgentKind/[agents.<id>].
    assert_eq!(SettingsTab::ALL[7].label(), "Providers");
}

#[test]
fn settings_tab_all_includes_modes_at_index_eight() {
    assert_eq!(SettingsTab::ALL[8], SettingsTab::Modes);
    assert_eq!(SettingsTab::ALL[8].label(), "Modes");
}

#[test]
fn agents_tab_has_default_provider_dropdown_and_dynamic_map() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let fields = &screen.fields_per_tab[7];
    assert!(
        fields.len() >= 2,
        "Providers tab (idx 7) must have at least the Default-provider Dropdown + DynamicMap entries"
    );
    assert!(
        matches!(fields[0].widget, WidgetKind::Dropdown(_)),
        "field[0] must be the Default-provider Dropdown, got {:?}",
        fields[0].widget.label()
    );
    assert!(
        matches!(fields[1].widget, WidgetKind::DynamicMap(_)),
        "field[1] must be the entries DynamicMap, got {:?}",
        fields[1].widget.label()
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
fn agents_default_dropdown_sync_writes_to_config() {
    // Build a config with two agent entries so the Default-provider
    // dropdown has more than one option to select between.
    let toml_str = "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n\
[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
[github]\n[notifications]\n[agents]\ndefault = \"claude\"\n\
[agents.claude]\nkind = \"claude\"\ncommand = \"claude\"\n\
[agents.opencode]\nkind = \"opencode\"\ncommand = \"opencode\"\n";
    let config: Config = toml::from_str(toml_str).expect("parse");
    let mut screen = SettingsScreen::new(config, make_flags());
    assert_eq!(screen.config.agents.default, "claude");
    // Flip the Dropdown to the "opencode" option, then sync.
    let fields = &mut screen.fields_per_tab[7];
    let WidgetKind::Dropdown(ref mut d) = fields[0].widget else {
        panic!("field[0] must be the Default-provider Dropdown");
    };
    let idx = d
        .options
        .iter()
        .position(|s| s == "opencode")
        .expect("`opencode` option must exist");
    d.selected = idx;
    screen.sync_widgets_to_config();
    assert_eq!(screen.config.agents.default, "opencode");
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
fn providers_tab_up_arrow_walks_back_through_dynamic_map_entry_fields() {
    // Repro for the navigation bug where pressing Up while focused on an
    // entry field inside the Providers DynamicMap jumped straight to the
    // Default-provider dropdown above, skipping every other entry field
    // and trapping the user — they could walk Down through the field list
    // but couldn't walk back Up to fix earlier values.
    let toml_str = "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n\
[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
[github]\n[notifications]\n[agents]\ndefault = \"claude\"\n\
[agents.claude]\nkind = \"claude\"\ncommand = \"claude\"\n";
    let config: Config = toml::from_str(toml_str).expect("parse");
    let mut screen = SettingsScreen::new(config, make_flags());
    screen.jump_to_tab(SettingsTab::Agents);
    assert_eq!(screen.field_index, 0, "Providers tab opens on the dropdown");

    // Down from the dropdown lands on the DynamicMap with focus=SubtabStrip.
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.field_index, 1);

    // Two more Downs step into the entry-field rows.
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.field_index, 1);
    {
        let WidgetKind::DynamicMap(dm) = &screen.fields_per_tab[7][1].widget else {
            panic!("expected DynamicMap at idx 1");
        };
        assert!(
            matches!(dm.focus(), MapFocus::EntryField(_)),
            "after two Downs inside DynamicMap focus must be on an EntryField, got {:?}",
            dm.focus()
        );
    }

    // Up walks back through entry fields BEFORE escaping to the dropdown.
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(
        screen.field_index, 1,
        "Up at a deeper EntryField must keep focus inside the DynamicMap"
    );

    // Another Up lands on SubtabStrip; still inside the DynamicMap.
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.field_index, 1);
    {
        let WidgetKind::DynamicMap(dm) = &screen.fields_per_tab[7][1].widget else {
            panic!("expected DynamicMap at idx 1");
        };
        assert_eq!(dm.focus(), &MapFocus::SubtabStrip);
    }

    // Only once focus is at the SubtabStrip boundary does Up escape to the dropdown.
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(
        screen.field_index, 0,
        "Up at SubtabStrip top boundary must exit DynamicMap to the dropdown"
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
