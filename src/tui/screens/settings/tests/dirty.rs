use super::*;

// --- Issue #74: Dirty state tests ---

#[test]
fn initially_not_dirty() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    assert!(!screen.is_dirty());
}

#[test]
fn modify_makes_dirty() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Toggle desktop notification
    for _ in 0..4 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(screen.is_dirty());
}

#[test]
fn ctrl_r_resets_dirty() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    let orig_desktop = screen.config.notifications.desktop;
    // Modify
    for _ in 0..4 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
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
    assert_eq!(screen.config.notifications.desktop, orig_desktop);
}

#[test]
fn esc_with_dirty_shows_confirmation() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Modify
    for _ in 0..4 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(screen.is_dirty());
    // Esc should trigger confirmation, not pop
    let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert!(screen.confirm_discard);
}

#[test]
fn confirm_discard_y_pops() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.confirm_discard = true;
    let action = screen.handle_input(&key_event(KeyCode::Char('y')), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
}

#[test]
fn confirm_discard_n_cancels() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.confirm_discard = true;
    let action = screen.handle_input(&key_event(KeyCode::Char('n')), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert!(!screen.confirm_discard);
}

#[test]
fn ctrl_s_saves_and_returns_update_config() {
    let (mut screen, _f) = screen_with_config_path();
    // Modify
    for _ in 0..4 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(screen.is_dirty());
    // Save
    let ctrl_s = Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    let action = screen.handle_input(&ctrl_s, InputMode::Normal);
    assert!(!screen.is_dirty()); // original updated
    assert!(matches!(action, ScreenAction::UpdateConfig(_)));
}

#[test]
fn ctrl_s_writes_to_file() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
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
    // Modify desktop notifications
    for _ in 0..4 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    // Save
    let ctrl_s = Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    screen.handle_input(&ctrl_s, InputMode::Normal);
    // Reload and verify
    let reloaded = Config::load(f.path()).unwrap();
    assert!(!reloaded.notifications.desktop);
}
