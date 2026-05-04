use super::*;

fn render_settings_to_string(screen: &mut SettingsScreen, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// Return the keybinds bar row — the second-to-last line of the rendered
/// output. The final line is the outer block's bottom border.
fn keybinds_row(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines
        .get(lines.len().saturating_sub(2))
        .copied()
        .unwrap_or("")
        .to_string()
}

#[test]
fn keybind_bar_project_text_input_shows_enter_edit() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    let output = render_settings_to_string(&mut screen, 80, 10);
    let row = keybinds_row(&output);
    assert!(
        row.contains("Enter"),
        "expected 'Enter' in keybinds row: {row}"
    );
    assert!(
        row.contains("Edit"),
        "expected 'Edit' in keybinds row: {row}"
    );
}

#[test]
fn keybind_bar_turboquant_toggle_shows_space_toggle() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    for _ in 0..10 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::TurboQuant);
    assert_eq!(screen.field_index, 0);
    let output = render_settings_to_string(&mut screen, 80, 10);
    let row = keybinds_row(&output);
    assert!(
        row.contains("Space"),
        "expected 'Space' in keybinds row: {row}"
    );
    assert!(
        row.contains("Toggle"),
        "expected 'Toggle' in keybinds row: {row}"
    );
}

#[test]
fn keybind_bar_turboquant_dropdown_shows_arrows_change() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    for _ in 0..10 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.field_index, 2);
    let output = render_settings_to_string(&mut screen, 80, 10);
    let row = keybinds_row(&output);
    assert!(row.contains("←/→"), "expected '←/→' in keybinds row: {row}");
    assert!(
        row.contains("Change"),
        "expected 'Change' in keybinds row: {row}"
    );
}

#[test]
fn keybind_bar_flags_tab_has_no_widget_hints() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    for _ in 0..9 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::Flags);
    let output = render_settings_to_string(&mut screen, 80, 10);
    let row = keybinds_row(&output);
    assert!(
        !row.contains("Space"),
        "Flags bar must not contain 'Space': {row}"
    );
    assert!(
        !row.contains("Change"),
        "Flags bar must not contain 'Change': {row}"
    );
}

#[test]
fn keybind_bar_list_editor_still_shows_save_esc_at_80_cols() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    for _ in 0..11 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::Advanced);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.field_index, 2);
    let output = render_settings_to_string(&mut screen, 80, 10);
    let row = keybinds_row(&output);
    assert!(
        row.contains("Ctrl+s"),
        "expected 'Ctrl+s' in keybinds row: {row}"
    );
    assert!(row.contains("Esc"), "expected 'Esc' in keybinds row: {row}");
}

#[test]
fn keybindings_includes_edit_group() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let groups = screen.keybindings();
    assert!(
        groups.len() >= 3,
        "expected at least 3 keybinding groups, got {}",
        groups.len()
    );
    let has_edit = groups.iter().any(|g| g.title == "Edit");
    assert!(has_edit, "expected a group titled 'Edit' in keybindings");
}

// --- Issue #437: config_path-required save + save_error_flash ---

fn ctrl_s_event() -> Event {
    Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn dirty_screen(screen: &mut SettingsScreen) {
    for _ in 0..4 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(screen.is_dirty(), "pre-condition: screen must be dirty");
}

#[test]
fn save_config_errors_when_no_config_path() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    dirty_screen(&mut screen);

    let result = screen.save_config();

    assert!(
        result.is_err(),
        "save_config must return Err when config_path is None"
    );
    assert!(
        screen.save_flash.is_none(),
        "save_flash must remain None on failure"
    );
    assert!(
        screen.is_dirty(),
        "is_dirty must remain true after failed save"
    );
}

#[test]
fn save_config_success_sets_flash_and_clears_dirty() {
    let (mut screen, _f) = screen_with_config_path();
    dirty_screen(&mut screen);

    let result = screen.save_config();

    assert!(result.is_ok(), "save_config must succeed with a valid path");
    assert!(
        screen.save_flash.is_some(),
        "save_flash must be set after successful save"
    );
    assert!(!screen.is_dirty(), "is_dirty must be false after save");
}

#[test]
fn ctrl_s_without_config_path_sets_error_flash() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    dirty_screen(&mut screen);

    let action = screen.handle_input(&ctrl_s_event(), InputMode::Normal);

    assert!(
        matches!(action, ScreenAction::None),
        "must return None when save fails, got {:?}",
        action
    );
    assert!(
        screen.save_error_flash.is_some(),
        "save_error_flash must be set after failed Ctrl+S"
    );
    assert!(
        screen.save_flash.is_none(),
        "success flash must stay absent on failure"
    );
}

#[test]
fn ctrl_s_with_valid_config_path_returns_update_config() {
    let (mut screen, _f) = screen_with_config_path();
    dirty_screen(&mut screen);

    let action = screen.handle_input(&ctrl_s_event(), InputMode::Normal);

    assert!(
        matches!(action, ScreenAction::UpdateConfig(_)),
        "must return UpdateConfig on successful save, got {:?}",
        action
    );
    assert!(!screen.is_dirty());
}

#[test]
fn save_error_flash_title_renders_with_error_style() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.save_error_flash = Some(("no path".into(), std::time::Instant::now()));

    let output = render_settings_to_string(&mut screen, 80, 10);
    let first_row = output.lines().next().unwrap_or("");
    assert!(
        first_row.contains("Save failed"),
        "title row must contain 'Save failed', got: {first_row:?}"
    );
}

#[test]
fn save_error_flash_expires_after_5_seconds() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.save_error_flash = Some((
        "x".into(),
        std::time::Instant::now() - std::time::Duration::from_secs(6),
    ));

    let output = render_settings_to_string(&mut screen, 80, 10);
    let first_row = output.lines().next().unwrap_or("");
    assert!(
        !first_row.contains("Save failed"),
        "expired flash must NOT appear in title, got: {first_row:?}"
    );
}

// --- Issue #490: caveman_mode toggle on Advanced tab ---

fn screen_with_caveman(state: CavemanModeState) -> SettingsScreen {
    SettingsScreen::new(make_config(), make_flags()).with_caveman_mode(state)
}

fn navigate_to_advanced_caveman_row(screen: &mut SettingsScreen) {
    for _ in 0..11 {
        screen.handle_input(
            &crate::tui::screens::test_helpers::key_event(KeyCode::Tab),
            InputMode::Normal,
        );
    }
    assert_eq!(screen.active_tab(), SettingsTab::Advanced);
    for _ in 0..3 {
        screen.handle_input(
            &crate::tui::screens::test_helpers::key_event(KeyCode::Down),
            InputMode::Normal,
        );
    }
    assert_eq!(screen.field_index, 3, "cursor must be on caveman row");
}

#[test]
fn caveman_keybinding_appears_in_help_overlay() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let groups = screen.keybindings();
    let descriptions: Vec<&str> = groups
        .iter()
        .flat_map(|g| g.bindings.iter().map(|b| b.description))
        .collect();
    assert!(
        descriptions
            .iter()
            .any(|d| d.to_lowercase().contains("caveman_mode")),
        "expected a caveman_mode binding, got: {descriptions:?}"
    );
}

#[test]
fn space_on_advanced_caveman_row_sets_pending_toggle() {
    let mut screen = screen_with_caveman(CavemanModeState::ExplicitFalse);
    navigate_to_advanced_caveman_row(&mut screen);
    screen.handle_input(
        &crate::tui::screens::test_helpers::key_event(KeyCode::Char(' ')),
        InputMode::Normal,
    );
    assert_eq!(screen.take_pending_caveman_toggle(), Some(true));
}

#[test]
fn space_on_explicit_true_sets_pending_to_false() {
    let mut screen = screen_with_caveman(CavemanModeState::ExplicitTrue);
    navigate_to_advanced_caveman_row(&mut screen);
    screen.handle_input(
        &crate::tui::screens::test_helpers::key_event(KeyCode::Char(' ')),
        InputMode::Normal,
    );
    assert_eq!(screen.take_pending_caveman_toggle(), Some(false));
}

#[test]
fn space_when_state_is_error_does_not_enqueue_pending_toggle() {
    let mut screen = screen_with_caveman(CavemanModeState::Error("read fail".into()));
    navigate_to_advanced_caveman_row(&mut screen);
    screen.handle_input(
        &crate::tui::screens::test_helpers::key_event(KeyCode::Char(' ')),
        InputMode::Normal,
    );
    assert_eq!(screen.take_pending_caveman_toggle(), None);
}

#[test]
fn space_when_state_is_error_shows_status_explanation() {
    let mut screen = screen_with_caveman(CavemanModeState::Error("read fail".into()));
    navigate_to_advanced_caveman_row(&mut screen);
    screen.handle_input(
        &crate::tui::screens::test_helpers::key_event(KeyCode::Char(' ')),
        InputMode::Normal,
    );
    let flash = screen
        .caveman_status_flash
        .as_ref()
        .expect("status flash should be set when toggling on Error");
    assert!(
        flash.0.to_lowercase().contains("unreadable"),
        "expected explanation to mention 'unreadable', got: {:?}",
        flash.0
    );
}

#[test]
fn set_caveman_state_updates_underlying_widget_value() {
    let mut screen = screen_with_caveman(CavemanModeState::ExplicitFalse);
    screen.set_caveman_state(CavemanModeState::ExplicitTrue);
    let advanced = &screen.fields_per_tab[11];
    let toggle = advanced
        .iter()
        .find_map(|f| match &f.widget {
            WidgetKind::Toggle(t) if t.label == "caveman_mode" => Some(t),
            _ => None,
        })
        .expect("caveman toggle exists in Advanced tab");
    assert!(toggle.value);
}
