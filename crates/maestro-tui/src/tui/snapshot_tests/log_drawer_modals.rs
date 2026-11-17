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
        let render = |open: bool| {
            let mut app = log_app(mode);
            app.nav_stack.push(TuiMode::IssueBrowser);
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
