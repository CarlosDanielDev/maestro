use super::*;

#[test]
fn initial_tab_is_project() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    assert_eq!(screen.active_tab(), SettingsTab::Project);
}

#[test]
fn tab_cycles_right() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert_eq!(screen.active_tab(), SettingsTab::Sessions);
}

#[test]
fn tab_wraps_right() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    for _ in 0..SettingsTab::ALL.len() {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::Project);
}

#[test]
fn tab_wraps_left() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key_event(KeyCode::BackTab), InputMode::Normal);
    assert_eq!(screen.active_tab(), SettingsTab::Advanced);
}

#[test]
fn field_navigation() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    assert_eq!(screen.field_index, 0);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.field_index, 1);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.field_index, 0);
}

// --- #505: Reset Settings (re-detect project stack) ---

#[test]
fn project_tab_contains_reset_settings_label() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let labels: Vec<&str> = screen.fields_per_tab[0]
        .iter()
        .map(|f| f.widget.label())
        .collect();
    assert!(
        labels.iter().any(|l| l.starts_with("Reset Settings")),
        "Project tab must include a 'Reset Settings' action; got {:?}",
        labels
    );
}

#[test]
fn reset_settings_row_returns_action_on_enter() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    let reset_idx = screen.fields_per_tab[0]
        .iter()
        .position(|f| f.widget.label().starts_with("Reset Settings"))
        .expect("Reset Settings row exists");
    screen.field_index = reset_idx;
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::ResetSettingsFromDetection);
}

#[test]
fn esc_returns_pop() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
}

#[test]
fn tab_switch_resets_field_index() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert!(screen.field_index > 0);
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert_eq!(screen.field_index, 0);
}

#[test]
fn toggle_widget_changes_config() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Navigate to Notifications tab (index 4)
    for _ in 0..4 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::Notifications);
    // First field is "desktop" (Toggle, default true)
    assert!(screen.config.notifications.desktop);
    // Toggle it
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(!screen.config.notifications.desktop);
}

#[test]
fn number_stepper_changes_config() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Navigate to Sessions tab
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert_eq!(screen.active_tab(), SettingsTab::Sessions);
    // First field is max_concurrent (NumberStepper, default 3)
    let orig = screen.config.sessions.max_concurrent;
    // Increment
    screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
    assert_eq!(screen.config.sessions.max_concurrent, orig + 1);
}

#[test]
fn dropdown_cycles_config() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Navigate to GitHub tab (index 3)
    for _ in 0..3 {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    // Navigate to merge_method (last field, index 4)
    for _ in 0..4 {
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    }
    // Default is squash (index 1), cycle right to rebase (index 2)
    screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
    assert_eq!(
        screen.config.github.merge_method,
        crate::config::MergeMethod::Rebase
    );
}

#[test]
fn desired_input_mode_normal_by_default() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    assert_eq!(screen.desired_input_mode(), Some(InputMode::Normal));
}

#[test]
fn keybindings_returns_non_empty() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let groups = screen.keybindings();
    assert!(!groups.is_empty());
}

#[test]
fn all_tabs_have_fields_except_flags() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    for (i, tab) in SettingsTab::ALL.iter().enumerate() {
        if *tab == SettingsTab::Flags {
            assert!(
                screen.fields_per_tab[i].is_empty(),
                "Flags tab must have no widget fields"
            );
        } else {
            assert!(
                !screen.fields_per_tab[i].is_empty(),
                "Tab {:?} has no fields",
                tab
            );
        }
    }
}
