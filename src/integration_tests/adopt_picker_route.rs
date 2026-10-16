//! Adopt picker → pool → Interaction route (#1054).
//!
//! Drives the picker's key handling, then the dispatcher's
//! `handle_screen_action`, against the real `App` and `SessionPool`. The
//! provider is the in-process default from `make_test_app` — no PTY, no
//! transcript writes. Fixture rows use ids that the `--resume` allowlist
//! accepts; the live-session stat finds no transcript under the test HOME.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crossterm::event::KeyCode;

use crate::session::discovery::DiscoveredSession;
use crate::session::types::SessionMode;
use crate::tui::app::{App, TuiMode};
use crate::tui::navigation::InputMode;
use crate::tui::screen_dispatch::handle_screen_action;
use crate::tui::screens::{AdoptPickerScreen, ScreenAction};

/// A real directory: `create_adopted_session` refuses cwds that are not dirs.
const REPO_CWD: &str = env!("CARGO_MANIFEST_DIR");

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
}

fn row(id: &str, cwd: &str, title: &str, age_secs: u64) -> DiscoveredSession {
    DiscoveredSession {
        session_id: id.to_string(),
        cwd: PathBuf::from(cwd),
        title: title.to_string(),
        last_active: now() - Duration::from_secs(age_secs),
        skipped_lines: 0,
    }
}

fn key(code: KeyCode) -> crossterm::event::Event {
    crate::tui::screens::test_helpers::key_event(code)
}

fn press(picker: &mut AdoptPickerScreen, code: KeyCode) -> ScreenAction {
    use crate::tui::screens::Screen;
    picker.handle_input(&key(code), InputMode::Insert)
}

fn adopted_sessions(app: &App) -> Vec<&crate::session::types::Session> {
    app.pool
        .all_sessions()
        .into_iter()
        .filter(|s| s.adopted_cwd.is_some())
        .collect()
}

/// Adopt the row under the cursor and hand the action to the dispatcher,
/// the same two steps the event loop runs for a picker Enter.
fn adopt_via_dispatcher(app: &mut App, action: ScreenAction) {
    assert!(
        matches!(action, ScreenAction::AdoptSession { .. }),
        "expected AdoptSession, got {action:?}"
    );
    handle_screen_action(app, action);
}

#[test]
fn adopt_creates_interactive_session_and_routes_to_interaction() {
    let mut app = crate::tui::make_test_app("adopt-picker-route");
    let mut picker = AdoptPickerScreen::new(
        vec![row("ext-stale-1", REPO_CWD, "fix the parser", 86_400)],
        now(),
    );

    let action = press(&mut picker, KeyCode::Enter);
    adopt_via_dispatcher(&mut app, action);

    assert_eq!(app.tui_mode, TuiMode::Interaction);
    assert!(app.screen_state.interaction_screen.is_some());

    let adopted = adopted_sessions(&app);
    assert_eq!(adopted.len(), 1, "exactly one adopted session in the pool");
    let session = adopted[0];
    assert_eq!(session.agent_session_id.as_deref(), Some("ext-stale-1"));
    assert_eq!(
        session.adopted_cwd.as_deref(),
        Some(PathBuf::from(REPO_CWD).as_path())
    );
    assert_eq!(session.session_mode, SessionMode::Interactive);
    assert!(
        session.worktree_path.is_none(),
        "adopted sessions never get a worktree"
    );
    assert_eq!(session.issue_title.as_deref(), Some("fix the parser"));
}

#[test]
fn picker_live_row_confirms_before_adopt() {
    let mut app = crate::tui::make_test_app("adopt-picker-live");
    let mut picker =
        AdoptPickerScreen::new(vec![row("ext-live-1", REPO_CWD, "still typing", 30)], now());

    // Enter on a live row opens the confirm modal and adopts nothing yet.
    assert_eq!(press(&mut picker, KeyCode::Enter), ScreenAction::None);
    assert!(picker.confirm_pending());
    assert!(adopted_sessions(&app).is_empty());
    assert_eq!(app.tui_mode, TuiMode::Overview, "still not routed");

    // Cancel keeps the picker open and the pool untouched.
    assert_eq!(press(&mut picker, KeyCode::Char('n')), ScreenAction::None);
    assert!(!picker.confirm_pending());
    assert!(adopted_sessions(&app).is_empty());

    // Confirm adopts and routes.
    press(&mut picker, KeyCode::Enter);
    let action = press(&mut picker, KeyCode::Char('y'));
    adopt_via_dispatcher(&mut app, action);

    assert_eq!(app.tui_mode, TuiMode::Interaction);
    let adopted = adopted_sessions(&app);
    assert_eq!(adopted.len(), 1);
    assert_eq!(adopted[0].agent_session_id.as_deref(), Some("ext-live-1"));
}

/// A refused cwd must not navigate away, and the refusal must show on the
/// picker itself (the activity log is not rendered by this screen).
#[test]
fn refused_cwd_keeps_the_picker_open_with_a_visible_status() {
    let mut app = crate::tui::make_test_app("adopt-picker-refused");
    let missing = format!("{REPO_CWD}/no-such-dir-1054b");
    app.screen_state.adopt_picker_screen = Some(AdoptPickerScreen::new(
        vec![row("ext-missing-1", &missing, "gone", 86_400)],
        now(),
    ));
    let picker = app
        .screen_state
        .adopt_picker_screen
        .as_mut()
        .expect("picker was just set");
    let action = press(picker, KeyCode::Enter);

    adopt_via_dispatcher(&mut app, action);

    assert_ne!(app.tui_mode, TuiMode::Interaction);
    assert!(adopted_sessions(&app).is_empty());
    let status = app
        .screen_state
        .adopt_picker_screen
        .as_ref()
        .and_then(|p| p.status())
        .expect("refusal must set a visible status on the picker");
    assert!(status.contains("not a directory"), "status was: {status}");
}
