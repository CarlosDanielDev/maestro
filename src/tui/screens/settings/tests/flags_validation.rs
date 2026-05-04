use super::*;

// --- Issue #146: Feature flags display tests ---

#[test]
fn feature_flags_tab_exists_in_all() {
    assert!(
        SettingsTab::ALL.contains(&SettingsTab::Flags),
        "Flags tab must be in ALL"
    );
}

#[test]
fn feature_flags_tab_label_is_flags() {
    assert_eq!(SettingsTab::Flags.label(), "Flags");
}

#[test]
fn flags_tab_has_no_widget_fields() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let flags_idx = SettingsTab::ALL
        .iter()
        .position(|t| *t == SettingsTab::Flags)
        .unwrap();
    assert!(screen.fields_per_tab[flags_idx].is_empty());
}

#[test]
fn flags_navigation_up_down() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Navigate to Flags tab
    let flags_idx = SettingsTab::ALL
        .iter()
        .position(|t| *t == SettingsTab::Flags)
        .unwrap();
    for _ in 0..flags_idx {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::Flags);
    assert_eq!(screen.flags_selected, 0);

    // Down
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.flags_selected, 1);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.flags_selected, 2);

    // Up
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.flags_selected, 1);

    // Up at 0 stays at 0
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.flags_selected, 0);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.flags_selected, 0);
}

#[test]
fn flags_navigation_bounded_by_flag_count() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    let flags_idx = SettingsTab::ALL
        .iter()
        .position(|t| *t == SettingsTab::Flags)
        .unwrap();
    for _ in 0..flags_idx {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    // Press Down more times than there are flags
    for _ in 0..20 {
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    }
    let max = crate::flags::Flag::all().len() - 1;
    assert_eq!(screen.flags_selected, max);
}

#[test]
fn flags_tab_read_only_ignores_widget_keys() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    let flags_idx = SettingsTab::ALL
        .iter()
        .position(|t| *t == SettingsTab::Flags)
        .unwrap();
    for _ in 0..flags_idx {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    // Space, Enter, 'l' should all be no-ops
    let action = screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn advanced_tab_still_works_after_flags_reindex() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Navigate to Advanced tab (last)
    let adv_idx = SettingsTab::ALL
        .iter()
        .position(|t| *t == SettingsTab::Advanced)
        .unwrap();
    for _ in 0..adv_idx {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }
    assert_eq!(screen.active_tab(), SettingsTab::Advanced);
    assert!(screen.field_count() > 0, "Advanced tab must have fields");

    // Modify heavy_task_limit
    let orig = screen.config.concurrency.heavy_task_limit;
    screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
    assert_eq!(screen.config.concurrency.heavy_task_limit, orig + 1);
}

#[test]
fn feature_flags_with_mixed_sources() {
    use std::collections::HashMap;
    let mut config_flags = HashMap::new();
    config_flags.insert("ci_auto_fix".to_string(), true);
    let flags = TestFeatureFlags::new(config_flags, vec!["model_routing".to_string()], vec![]);
    let screen = SettingsScreen::new(make_config(), flags);
    let entries = screen.feature_flags.all_with_source();

    let ci = entries
        .iter()
        .find(|(f, _, _)| *f == crate::flags::Flag::CiAutoFix)
        .unwrap();
    assert!(ci.1);
    assert_eq!(ci.2, crate::flags::FlagSource::Config);

    let mr = entries
        .iter()
        .find(|(f, _, _)| *f == crate::flags::Flag::ModelRouting)
        .unwrap();
    assert!(mr.1);
    assert_eq!(mr.2, crate::flags::FlagSource::Cli);
}

// --- Issue #75: Field-level validation tests ---

#[test]
fn valid_config_has_no_validation_errors() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    assert!(!screen.has_validation_errors());
}

#[test]
fn validation_runs_on_field_change() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    assert!(!screen.has_validation_errors());
    // Navigate to Project tab, field 0 (repo), enter edit mode, clear value
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    // Select all and delete
    screen.handle_input(&key_event(KeyCode::Home), InputMode::Normal);
    // Delete all chars
    for _ in 0..20 {
        screen.handle_input(&key_event(KeyCode::Delete), InputMode::Normal);
    }
    screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert!(screen.has_validation_errors());
}

#[test]
fn save_blocked_when_validation_errors_exist() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Make repo invalid
    screen.config.project.repo = String::new();
    screen.run_all_validations();
    assert!(screen.has_validation_errors());

    let ctrl_s = Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    let action = screen.handle_input(&ctrl_s, InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn save_with_validation_errors_populates_save_error_flash() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.config.project.base_branch = String::new();
    screen.run_all_validations();
    assert!(screen.has_validation_errors());
    assert!(screen.save_error_flash.is_none());

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
        .expect("save_error_flash must be set when validation blocks the save");
    assert!(
        flash.0.to_lowercase().contains("base_branch"),
        "flash message must name the failing field, got: {:?}",
        flash.0
    );
}

#[test]
fn save_with_no_validation_errors_does_not_set_error_flash() {
    let (mut screen, _f) = screen_with_config_path();
    assert!(!screen.has_validation_errors());

    let ctrl_s = Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    screen.handle_input(&ctrl_s, InputMode::Normal);

    assert!(
        screen.save_error_flash.is_none(),
        "valid save must not set save_error_flash"
    );
}

#[test]
fn save_allowed_when_no_validation_errors() {
    let (mut screen, _f) = screen_with_config_path();
    assert!(!screen.has_validation_errors());

    let ctrl_s = Event::Key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    let action = screen.handle_input(&ctrl_s, InputMode::Normal);
    assert!(matches!(action, ScreenAction::UpdateConfig(_)));
}

#[test]
fn feedback_for_returns_none_for_valid_field() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    assert!(screen.feedback_for(0, 0).is_none()); // repo is valid
}

#[test]
fn feedback_for_returns_error_for_invalid_field() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.config.project.repo = String::new();
    screen.run_all_validations();
    let fb = screen.feedback_for(0, 0);
    assert!(fb.is_some());
    assert!(fb.unwrap().is_error());
}

#[test]
fn cross_field_validation_ci_wait_vs_poll() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    screen.config.gates.ci_poll_interval_secs = 60;
    screen.config.gates.ci_max_wait_secs = 60;
    screen.run_all_validations();
    let fb = screen.feedback_for(5, 3);
    assert!(fb.is_some());
    assert!(fb.unwrap().is_error());
}

// --- Issue #275: hollow retry policy widgets in Sessions tab ---

#[test]
fn sessions_tab_contains_hollow_retry_widgets() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let fields = &screen.fields_per_tab[1];
    // Fields 8, 9, 10 are the three hollow_retry widgets (after
    // max_concurrent, stall_timeout_secs, default_model, default_mode,
    // bypass_review_corrections, permission_mode, max_retries,
    // retry_cooldown_secs).
    match &fields[8].widget {
        WidgetKind::Dropdown(d) => assert_eq!(d.label, "hollow_retry.policy"),
        _ => panic!("expected Dropdown at field 8 (hollow_retry.policy)"),
    }
    match &fields[9].widget {
        WidgetKind::NumberStepper(s) => {
            assert_eq!(s.label, "hollow_retry.work_max_retries")
        }
        _ => panic!("expected NumberStepper at field 9 (work_max_retries)"),
    }
    match &fields[10].widget {
        WidgetKind::NumberStepper(s) => {
            assert_eq!(s.label, "hollow_retry.consultation_max_retries")
        }
        _ => panic!("expected NumberStepper at field 10 (consultation_max_retries)"),
    }
}

#[test]
fn sessions_tab_hollow_retry_policy_defaults_to_intent_aware() {
    let screen = SettingsScreen::new(make_config(), make_flags());
    let fields = &screen.fields_per_tab[1];
    let WidgetKind::Dropdown(d) = &fields[8].widget else {
        panic!("field 8 must be Dropdown");
    };
    // Options order: [always, intent-aware, never] → default index 1.
    assert_eq!(d.selected, 1);
    assert_eq!(d.selected_value(), "intent-aware");
}

#[test]
fn sessions_tab_hollow_retry_sync_writes_policy_to_config() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    // Directly mutate the dropdown to "never" (index 2).
    if let Some(WidgetKind::Dropdown(d)) = screen
        .fields_per_tab
        .get_mut(1)
        .and_then(|fs| fs.get_mut(8))
        .map(|f| &mut f.widget)
    {
        d.selected = 2;
    }
    screen.sync_widgets_to_config();
    assert_eq!(
        screen.config.sessions.hollow_retry.policy,
        crate::config::HollowRetryPolicy::Never
    );
}

#[test]
fn sessions_tab_hollow_retry_sync_writes_steppers_to_config() {
    let mut screen = SettingsScreen::new(make_config(), make_flags());
    if let Some(WidgetKind::NumberStepper(s)) = screen
        .fields_per_tab
        .get_mut(1)
        .and_then(|fs| fs.get_mut(9))
        .map(|f| &mut f.widget)
    {
        s.value = 5;
    }
    if let Some(WidgetKind::NumberStepper(s)) = screen
        .fields_per_tab
        .get_mut(1)
        .and_then(|fs| fs.get_mut(10))
        .map(|f| &mut f.widget)
    {
        s.value = 3;
    }
    screen.sync_widgets_to_config();
    assert_eq!(screen.config.sessions.hollow_retry.work_max_retries, 5);
    assert_eq!(
        screen.config.sessions.hollow_retry.consultation_max_retries,
        3
    );
}
