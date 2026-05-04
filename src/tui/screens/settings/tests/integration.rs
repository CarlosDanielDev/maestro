use super::*;

// --- Issue #77: Integration tests ---

#[test]
fn integration_full_settings_flow_modify_save_reload() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 3
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
auto_pr = true
[notifications]
desktop = true
"#
    )
    .unwrap();
    let config = Config::load(f.path()).unwrap();
    let mut screen =
        SettingsScreen::new(config, make_flags()).with_config_path(f.path().to_path_buf());

    // Modify: sessions tab, increment max_concurrent
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal); // 3 -> 4
    assert_eq!(screen.config.sessions.max_concurrent, 4);
    assert!(screen.is_dirty());

    // Save
    let ctrl_s = Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    let action = screen.handle_input(&ctrl_s, InputMode::Normal);
    assert!(!screen.is_dirty());
    assert!(matches!(action, ScreenAction::UpdateConfig(_)));

    // Reload file and verify
    let reloaded = Config::load(f.path()).unwrap();
    assert_eq!(reloaded.sessions.max_concurrent, 4);
}

#[test]
fn integration_modify_esc_confirm_discard_verify_file_unchanged() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 3
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    )
    .unwrap();
    let config = Config::load(f.path()).unwrap();
    let mut screen =
        SettingsScreen::new(config, make_flags()).with_config_path(f.path().to_path_buf());

    // Modify
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
    assert!(screen.is_dirty());

    // Esc triggers confirmation
    let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert!(screen.confirm_discard);

    // Confirm discard
    let action = screen.handle_input(&key_event(KeyCode::Char('y')), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);

    // File should be unchanged
    let reloaded = Config::load(f.path()).unwrap();
    assert_eq!(reloaded.sessions.max_concurrent, 3);
}

#[test]
fn integration_modify_ctrl_r_verify_all_fields_reset() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    let orig = screen.config.clone();

    // Modify multiple things
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal); // Sessions
    screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal); // max_concurrent++

    for _ in 0..3 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    } // Notifications
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // toggle desktop

    assert!(screen.is_dirty());

    // Reset
    let ctrl_r = Event::Key(KeyEvent {
        code: KeyCode::Char('r'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    screen.handle_input(&ctrl_r, InputMode::Normal);
    assert!(!screen.is_dirty());
    assert_eq!(screen.config, orig);
}

#[test]
fn integration_theme_preview_on_change_emits_preview() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());

    // Go to Theme tab (index 7)
    for _ in 0..7 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::Theme);

    // First field is live_preview toggle (default off)
    assert!(!screen.live_preview);
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(screen.live_preview);

    // Move to preset dropdown
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    // Change preset — should emit PreviewTheme
    let action = screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
    assert!(matches!(action, ScreenAction::PreviewTheme(Some(_))));
}

#[test]
fn integration_theme_preview_reset_clears_preview() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.live_preview = true;

    let ctrl_r = Event::Key(KeyEvent {
        code: KeyCode::Char('r'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    let action = screen.handle_input(&ctrl_r, InputMode::Normal);
    assert!(matches!(action, ScreenAction::PreviewTheme(None)));
    assert!(!screen.live_preview);
}

#[test]
fn integration_layout_tab_fields() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Navigate to Layout tab (index 8)
    for _ in 0..8 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::Layout);
    assert_eq!(screen.field_count(), 4); // mode, density, preview_ratio, activity_log_height

    // Cycle mode from vertical to horizontal
    screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
    assert_eq!(
        screen.config.tui.layout.mode,
        crate::config::LayoutMode::Horizontal
    );
}

#[test]
fn integration_keybindings_grouped_logically() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let groups = screen.keybindings();
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].title, "Navigation");
    assert_eq!(groups[1].title, "Edit");
    assert_eq!(groups[2].title, "Actions");
    assert!(groups[0].bindings.len() >= 3);
    assert!(groups[1].bindings.len() >= 2);
    assert!(groups[2].bindings.len() >= 2);
}
