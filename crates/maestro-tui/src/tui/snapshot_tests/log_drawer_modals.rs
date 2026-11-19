//! The log drawer under modals (#1263): popups, confirm screens and
//! confirms inside a screen take keys before (or instead of) the global
//! `Ctrl+L` / `Esc`, so an open drawer is hidden under them.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::log_drawer::{draw, esc, log_app};
use crate::tui::app::TuiMode;
use crate::tui::input_handler::handle_key;

/// Modals take keys before (or instead of) the global `Ctrl+L` / `Esc`, so
/// an open drawer must never cover them: e.g. the BudgetPreSpawn spend
/// confirm still takes `y`/`n`.
#[test]
fn log_drawer_hidden_under_modals() {
    let nil = uuid::Uuid::nil();
    for mode in [
        TuiMode::BudgetPreSpawn { session_id: nil },
        TuiMode::ConfirmKill(nil),
        TuiMode::ConfirmExit,
        TuiMode::QueueExecution,
        TuiMode::CompletionSummary,
        TuiMode::ContinuousPause,
        TuiMode::HollowRetry,
        TuiMode::AdaptFollowUp,
        TuiMode::CiErrorReview,
        TuiMode::SessionSwitcher,
        TuiMode::BypassWarning,
        TuiMode::QueueConfirmation,
    ] {
        // One app, drawer toggled: two apps would seed log lines with two
        // wall-clock stamps that can differ by a second.
        let mut app = log_app(mode);
        app.nav_stack.push(TuiMode::IssueBrowser);
        let mut render = |open: bool| {
            app.activity_log.drawer_open = open;
            format!("{}", draw(&mut app, 80, 24).backend())
        };
        let closed = render(false);
        let open = render(true);
        assert_eq!(
            open, closed,
            "{mode:?}: the drawer must not cover the modal"
        );
    }
    let mut app = log_app(TuiMode::BudgetPreSpawn { session_id: nil });
    app.activity_log.drawer_open = true;
    let out = format!("{}", draw(&mut app, 80, 24).backend());
    assert!(out.contains("$"), "spend confirm text is visible:\n{out}");
    assert!(!out.contains("Activity Log ]"), "no drawer rows:\n{out}");
}

/// With the drawer closed, `Esc` reaches the screen (here: Settings pops).
#[tokio::test]
async fn esc_reaches_screen_when_drawer_closed() {
    let mut app = log_app(TuiMode::Settings);
    app.nav_stack.push(TuiMode::Landing);
    let _ = handle_key(&mut app, esc()).await;
    assert_ne!(app.tui_mode, TuiMode::Settings, "Esc goes to the screen");
}

/// Under a modal the hidden drawer does not eat `Esc`: it reaches the modal.
#[tokio::test]
async fn esc_reaches_modal_while_drawer_hidden() {
    let mut app = log_app(TuiMode::ConfirmKill(uuid::Uuid::nil()));
    app.nav_stack.push(TuiMode::Overview);
    app.activity_log.drawer_open = true;
    let _ = handle_key(&mut app, esc()).await;
    assert_ne!(
        app.tui_mode,
        TuiMode::ConfirmKill(uuid::Uuid::nil()),
        "Esc closes the kill confirm"
    );
}

/// Confirms drawn inside a screen hide the drawer too, and take `Esc`.
#[tokio::test]
async fn log_drawer_hidden_under_screen_confirms() {
    use crate::session::interaction::TurnState;
    use crate::tui::navigation::InputMode;
    use crate::tui::screens::settings::SettingsTab;
    use crate::tui::screens::test_helpers::key_event;
    use crate::tui::screens::{InteractionScreen, Screen};

    // Settings: an unsaved edit, then Esc asks "discard?".
    let settings = |open: bool| {
        let mut app = log_app(TuiMode::Settings);
        let screen = app.screen_state.settings_screen.as_mut().unwrap();
        screen.jump_to_tab(SettingsTab::Notifications);
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        app.activity_log.drawer_open = open;
        app
    };
    let mut app = settings(true);
    assert!(
        app.screen_state
            .settings_screen
            .as_ref()
            .unwrap()
            .confirm_discard_open()
    );
    let open = format!("{}", draw(&mut app, 80, 24).backend());
    let closed = format!("{}", draw(&mut settings(false), 80, 24).backend());
    assert_eq!(open, closed, "Settings discard confirm stays visible");
    let _ = handle_key(&mut app, esc()).await;
    assert!(
        !app.screen_state
            .settings_screen
            .as_ref()
            .unwrap()
            .confirm_discard_open(),
        "Esc reaches the discard confirm"
    );

    // Interaction: Ctrl+W opens the quit confirm.
    let interaction = |open: bool| {
        let mut app = log_app(TuiMode::Interaction);
        let mut screen = InteractionScreen::test_fixture(
            7,
            true,
            TurnState::Idle,
            vec![],
            "/tmp/maestro/wt-1263",
        );
        screen.handle_input(
            &crossterm::event::Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            InputMode::Insert,
        );
        app.screen_state.interaction_screen = Some(screen);
        app.activity_log.drawer_open = open;
        app
    };
    let mut app = interaction(true);
    assert!(
        app.screen_state
            .interaction_screen
            .as_ref()
            .unwrap()
            .quit_modal_is_open()
    );
    let open = format!("{}", draw(&mut app, 120, 40).backend());
    let closed = format!("{}", draw(&mut interaction(false), 120, 40).backend());
    assert_eq!(open, closed, "Interaction quit confirm stays visible");
}

/// The Adopt picker and Fleet board live confirms block the drawer.
#[test]
fn log_drawer_blocked_by_adopt_and_fleet_confirms() {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use crate::session::discovery::DiscoveredSession;
    use crate::tui::navigation::InputMode;
    use crate::tui::screens::Screen;
    use crate::tui::screens::adopt_picker::AdoptPickerScreen;
    use crate::tui::screens::fleet_board::FleetBoardScreen;
    use crate::tui::screens::fleet_board::rows::build_rows;
    use crate::tui::screens::test_helpers::key_event;
    use crate::tui::ui::log_drawer_blocked;

    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let live = || DiscoveredSession {
        session_id: "ext-1263".to_string(),
        cwd: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        title: "still typing".to_string(),
        last_active: now - Duration::from_secs(30),
        skipped_lines: 0,
    };

    let mut app = log_app(TuiMode::AdoptPicker);
    app.screen_state.adopt_picker_screen = Some(AdoptPickerScreen::new(vec![live()], now));
    assert!(!log_drawer_blocked(&app), "no confirm yet");
    let picker = app.screen_state.adopt_picker_screen.as_mut().unwrap();
    picker.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert!(picker.confirm_pending(), "setup: live confirm open");
    assert!(log_drawer_blocked(&app), "adopt live confirm blocks");

    let mut app = log_app(TuiMode::FleetBoard);
    app.screen_state.fleet_board_screen = Some(FleetBoardScreen::new(
        build_rows(&[], "maestro", &[live()]),
        now,
    ));
    assert!(!log_drawer_blocked(&app), "no confirm yet");
    let board = app.screen_state.fleet_board_screen.as_mut().unwrap();
    board.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert!(board.confirm_pending(), "setup: live confirm open");
    assert!(log_drawer_blocked(&app), "fleet live confirm blocks");
}

/// While shown, the drawer is modal: on Settings with the remove-entry
/// confirm open, `y` and Enter do nothing and Esc closes only the drawer.
#[tokio::test]
async fn shown_drawer_swallows_keys_over_settings_remove_confirm() {
    use crate::tui::navigation::InputMode;
    use crate::tui::screens::Screen;
    use crate::tui::screens::settings::schema_tab::widgets::dynamic_map::MapFocus;
    use crate::tui::screens::settings::{SettingsScreen, SettingsTab};
    use crate::tui::screens::test_helpers::key_event;
    use crate::tui::widgets::WidgetKind;

    let toml = "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n\
[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
[github]\n[notifications]\n[agents]\ndefault = \"claude\"\n\
[agents.claude]\nkind = \"claude\"\ncommand = \"claude\"\n";
    let mut app = log_app(TuiMode::Settings);
    let mut screen = SettingsScreen::new(toml::from_str(toml).unwrap(), app.flags.clone());
    screen.jump_to_tab(SettingsTab::Agents);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
    app.screen_state.settings_screen = Some(screen);
    let map = |app: &crate::tui::app::App| match &app
        .screen_state
        .settings_screen
        .as_ref()
        .unwrap()
        .fields_per_tab[7][1]
        .widget
    {
        WidgetKind::DynamicMap(dm) => (dm.entries().len(), dm.focus().clone()),
        _ => panic!("expected the providers DynamicMap"),
    };
    assert_eq!(
        map(&app),
        (1, MapFocus::RemoveConfirm),
        "setup: confirm open"
    );

    app.activity_log.drawer_open = true;
    assert!(crate::tui::ui::log_drawer_shown(&app));
    for code in [KeyCode::Char('y'), KeyCode::Enter] {
        let _ = handle_key(&mut app, KeyEvent::new(code, KeyModifiers::NONE)).await;
        assert_eq!(
            map(&app),
            (1, MapFocus::RemoveConfirm),
            "{code:?} must do nothing"
        );
    }
    let _ = handle_key(&mut app, esc()).await;
    assert!(!app.activity_log.drawer_open, "Esc closes the drawer");
    assert_eq!(
        map(&app),
        (1, MapFocus::RemoveConfirm),
        "only the drawer closed"
    );
    assert_eq!(app.tui_mode, TuiMode::Settings);
}

/// On the Issues browser with the launch prompt open, Enter under the
/// shown drawer does not launch.
#[tokio::test]
async fn shown_drawer_swallows_enter_over_issue_prompt() {
    use crate::tui::screens::issue_browser::{IssuePromptOverlay, LaunchFocus};

    let mut app = log_app(TuiMode::IssueBrowser);
    app.screen_state
        .issue_browser_screen
        .as_mut()
        .unwrap()
        .prompt_overlay = Some(IssuePromptOverlay {
        editor: IssuePromptOverlay::make_editor(""),
        selected_issues: vec![(1, "Add login flow".to_string())],
        unified_pr: false,
        focus: LaunchFocus::Launch,
        produce_pr: true,
        interaction: false,
    });
    app.activity_log.drawer_open = true;
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
    let screen = app.screen_state.issue_browser_screen.as_ref().unwrap();
    assert!(screen.prompt_overlay.is_some(), "Enter must not launch");
    assert!(app.pool.all_sessions().is_empty(), "no session launched");
    assert_eq!(app.tui_mode, TuiMode::IssueBrowser);
}

/// The footer shows only the drawer's keys while it is shown.
#[test]
fn shown_drawer_footer_has_only_drawer_hints() {
    let mut app = log_app(TuiMode::IssueBrowser);
    app.activity_log.drawer_open = true;
    let out = format!("{}", draw(&mut app, 80, 24).backend());
    let footer = out.lines().last().unwrap().to_string();
    assert!(
        footer.contains("Esc Close log") && footer.contains("⌃L close"),
        "{footer}"
    );
    assert!(
        !footer.contains("Enter"),
        "screen hints are hidden: {footer}"
    );
}
