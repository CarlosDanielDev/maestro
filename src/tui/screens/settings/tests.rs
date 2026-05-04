use super::*;
use crate::config::Config;
use crate::flags::store::FeatureFlags as TestFeatureFlags;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::KeymapProvider;
use crate::tui::screens::{Screen, ScreenAction, test_helpers::key_event};
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

fn make_flags() -> TestFeatureFlags {
    TestFeatureFlags::default()
}

fn make_config() -> Config {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write;
    write!(f, "{}", MINIMAL_SETTINGS_TOML).unwrap();
    Config::load(f.path()).unwrap()
}

const MINIMAL_SETTINGS_TOML: &str = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n";

/// Construct a `SettingsScreen` backed by a real tempfile so `Ctrl+s`
/// actually writes. The `NamedTempFile` must be kept alive by the caller
/// for the duration of the test — dropping it deletes the backing file.
fn screen_with_config_path() -> (SettingsScreen, tempfile::NamedTempFile) {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(f, "{}", MINIMAL_SETTINGS_TOML).unwrap();
    let config = Config::load(f.path()).unwrap();
    let screen = SettingsScreen::new(config, make_flags()).with_config_path(f.path().to_path_buf());
    (screen, f)
}

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
