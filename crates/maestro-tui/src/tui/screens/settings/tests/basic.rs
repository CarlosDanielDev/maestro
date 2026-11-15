use super::*;

#[test]
fn initial_tab_is_first_alphabetical() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    // Sidebar opens on the first alphabetical entry (Advanced) so the
    // initial selection matches what the user sees.
    assert_eq!(screen.active_tab(), SettingsTab::Advanced);
}

#[test]
fn tab_cycles_right_alphabetical() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    // Tab walks alphabetical order — Advanced → Agents.
    assert_eq!(screen.active_tab(), SettingsTab::Agents);
}

#[test]
fn tab_wraps_right_alphabetical() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    for _ in 0..SettingsTab::ALPHABETICAL_INDICES.len() {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::Advanced);
}

#[test]
fn tab_wraps_left_alphabetical() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.handle_input(&key_event(KeyCode::BackTab), InputMode::Normal);
    // From the first alphabetical tab (Advanced), BackTab wraps to the
    // last alphabetical (TurboQuant).
    assert_eq!(screen.active_tab(), SettingsTab::TurboQuant);
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
    screen.jump_to_tab(SettingsTab::Project);
    let reset_idx = screen.fields_per_tab[0]
        .iter()
        .position(|f| f.widget.label().starts_with("Reset Settings"))
        .expect("Reset Settings row exists");
    screen.field_index = reset_idx;
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::ResetSettingsFromDetection);
}

#[test]
fn project_tab_contains_normalize_agent_config_label() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let labels: Vec<&str> = screen.fields_per_tab[0]
        .iter()
        .map(|f| f.widget.label())
        .collect();
    assert!(
        labels
            .iter()
            .any(|l| l.starts_with("Normalize Agent Config")),
        "Project tab must include a 'Normalize Agent Config' action; got {:?}",
        labels
    );
}

#[test]
fn normalize_agent_config_row_returns_action_on_enter() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.jump_to_tab(SettingsTab::Project);
    let normalize_idx = screen.fields_per_tab[0]
        .iter()
        .position(|f| f.widget.label().starts_with("Normalize Agent Config"))
        .expect("Normalize Agent Config row exists");
    screen.field_index = normalize_idx;
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::NormalizeAgentConfig);
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
    screen.jump_to_tab(SettingsTab::Notifications);
    // First field is "desktop" (Toggle, default true)
    assert!(screen.config.notifications.desktop);
    // Toggle it
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(!screen.config.notifications.desktop);
}

#[test]
fn number_stepper_changes_config() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.jump_to_tab(SettingsTab::Sessions);
    // First field is max_concurrent (NumberStepper, default 3)
    let orig = screen.config.sessions.max_concurrent;
    // Increment
    screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
    assert_eq!(screen.config.sessions.max_concurrent, orig + 1);
}

#[test]
fn dropdown_cycles_config() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.jump_to_tab(SettingsTab::GitHub);
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
fn a_key_from_scalar_field_routes_to_tab_dynamic_widget() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Sessions tab hosts both scalar fields (max_concurrent, …) and the
    // `completion_gates.commands` DynamicRows widget at the end.
    screen.jump_to_tab(SettingsTab::Sessions);
    // Cursor starts on field 0 (max_concurrent, NumberStepper). Press `a`.
    let start_field = screen.field_index;
    screen.handle_input(&key_event(KeyCode::Char('a')), InputMode::Normal);
    // Expectation: field_index jumped onto the DynamicRows row AND the
    // widget opened its Add modal. We can't easily peek into the modal
    // from outside, but the focus jump is observable.
    let target_field = screen.field_index;
    assert_ne!(
        start_field, target_field,
        "pressing `a` while focused on a scalar field must move focus onto the tab's dynamic widget"
    );
    let target_widget = &screen.fields_per_tab[screen.active_tab][target_field].widget;
    assert!(
        matches!(
            target_widget,
            WidgetKind::DynamicRows(_) | WidgetKind::DynamicMap(_)
        ),
        "target field must be the dynamic-cardinality widget, got {:?}",
        target_widget.label()
    );
}

#[test]
fn alphabetical_indices_match_all_length() {
    assert_eq!(
        SettingsTab::ALPHABETICAL_INDICES.len(),
        SettingsTab::ALL.len(),
        "ALPHABETICAL_INDICES must have one entry per SettingsTab variant"
    );
}

#[test]
fn alphabetical_indices_are_unique_and_in_range() {
    let n = SettingsTab::ALL.len();
    let mut seen = vec![false; n];
    for &idx in SettingsTab::ALPHABETICAL_INDICES {
        assert!(
            idx < n,
            "ALPHABETICAL_INDICES contains out-of-range index {idx}"
        );
        assert!(
            !seen[idx],
            "ALPHABETICAL_INDICES contains duplicate index {idx}"
        );
        seen[idx] = true;
    }
}

#[test]
fn alphabetical_indices_produce_variant_name_sorted_order() {
    // Pre-existing convention: the sidebar sorts by VARIANT NAME, not by
    // displayed label. `Agents` has label "Providers" but sits between
    // Advanced and Budget in the alphabetical list. Locking this here so
    // a future label rename doesn't silently break the ordering.
    let variant_names: Vec<String> = SettingsTab::ALPHABETICAL_INDICES
        .iter()
        .map(|&i| format!("{:?}", SettingsTab::ALL[i]))
        .collect();
    let mut sorted = variant_names.clone();
    sorted.sort();
    assert_eq!(
        variant_names, sorted,
        "ALPHABETICAL_INDICES must be sorted by variant name"
    );
}

#[test]
fn teams_tab_is_registered_between_modes_and_theme() {
    let positions: Vec<(usize, SettingsTab)> = SettingsTab::ALL
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, t)| {
            matches!(
                t,
                SettingsTab::Modes | SettingsTab::Teams | SettingsTab::Theme
            )
        })
        .collect();
    assert_eq!(
        positions
            .iter()
            .map(|(_, t)| *t)
            .collect::<Vec<SettingsTab>>(),
        vec![SettingsTab::Modes, SettingsTab::Teams, SettingsTab::Theme],
        "Teams must sit between Modes and Theme in SettingsTab::ALL"
    );
    assert_eq!(SettingsTab::Teams.label(), "Teams");
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
