//! Fleet board actions (#1068): adopt an external row through the same
//! pipeline as the Adopt Session picker, and jump to a managed row without
//! creating a second session.
//!
//! Drives the board's key handling and the dispatcher's
//! `handle_screen_action` against the in-process `App`. No PTY, no
//! transcript writes; the adopted session uses a real directory so the
//! pool accepts its cwd.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crossterm::event::KeyCode;

use crate::session::discovery::DiscoveredSession;
use crate::session::types::SessionMode;
use crate::tui::app::{App, TuiMode};
use crate::tui::navigation::InputMode;
use crate::tui::screen_dispatch::handle_screen_action;
use crate::tui::screens::fleet_board::rows::build_rows;
use crate::tui::screens::fleet_board::{FleetBoardScreen, FleetRow, FleetSource};
use crate::tui::screens::{Screen, ScreenAction};

/// A real directory: `create_adopted_session` refuses cwds that are not dirs.
const REPO_CWD: &str = env!("CARGO_MANIFEST_DIR");
const EXTERNAL_ID: &str = "ext-fleet-action-1";

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
}

fn transcript(id: &str, cwd: &str, title: &str, age_secs: u64) -> DiscoveredSession {
    DiscoveredSession {
        session_id: id.to_string(),
        cwd: PathBuf::from(cwd),
        title: title.to_string(),
        last_active: now() - Duration::from_secs(age_secs),
        skipped_lines: 0,
    }
}

fn press(board: &mut FleetBoardScreen, code: KeyCode) -> ScreenAction {
    board.handle_input(
        &crate::tui::screens::test_helpers::key_event(code),
        InputMode::Insert,
    )
}

/// Put `rows` on a board and make it the active screen, as the Fleet board
/// route does.
fn open_board(app: &mut App, rows: Vec<FleetRow>) {
    app.tui_mode = TuiMode::FleetBoard;
    app.screen_state.fleet_board_screen = Some(FleetBoardScreen::new(rows, now()));
}

fn board(app: &mut App) -> &mut FleetBoardScreen {
    app.screen_state
        .fleet_board_screen
        .as_mut()
        .expect("board was opened")
}

/// Move the cursor onto the first row matching `pick`.
fn select(board: &mut FleetBoardScreen, pick: impl Fn(&FleetRow) -> bool) {
    for _ in 0..board.filtered_rows().len() {
        if board.selected_row().is_some_and(&pick) {
            return;
        }
        press(board, KeyCode::Down);
    }
    panic!("no row matched the selector");
}

fn adopted_sessions(app: &App) -> Vec<&crate::session::types::Session> {
    app.pool
        .all_sessions()
        .into_iter()
        .filter(|s| s.adopted_cwd.is_some())
        .collect()
}

#[test]
fn fleet_adopt_reuses_picker_pipeline() {
    let mut app = crate::tui::make_test_app("fleet-action-adopt");
    let rows = build_rows(
        &[],
        "maestro",
        &[transcript(EXTERNAL_ID, REPO_CWD, "fix the parser", 86_400)],
    );
    open_board(&mut app, rows);

    // A stale external row (not live) adopts with no confirm step.
    let action = press(board(&mut app), KeyCode::Enter);
    assert!(
        matches!(action, ScreenAction::AdoptSession { .. }),
        "expected the picker's AdoptSession action, got {action:?}"
    );
    handle_screen_action(&mut app, action);

    assert_eq!(app.tui_mode, TuiMode::Interaction);
    let adopted = adopted_sessions(&app);
    assert_eq!(adopted.len(), 1, "exactly one adopted session in the pool");
    let session = adopted[0];
    assert_eq!(session.agent_session_id.as_deref(), Some(EXTERNAL_ID));
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
fn fleet_live_row_confirms_and_defaults_to_cancel() {
    let mut app = crate::tui::make_test_app("fleet-action-live");
    let rows = build_rows(
        &[],
        "maestro",
        &[transcript(EXTERNAL_ID, REPO_CWD, "still typing", 30)],
    );
    open_board(&mut app, rows);

    // Enter on a live row opens the confirm modal and adopts nothing yet.
    assert_eq!(press(board(&mut app), KeyCode::Enter), ScreenAction::None);
    assert!(board(&mut app).confirm_pending());
    assert!(adopted_sessions(&app).is_empty());
    assert_eq!(app.tui_mode, TuiMode::FleetBoard, "still on the board");

    // A second Enter is not a yes: it cancels, adopts nothing.
    assert_eq!(press(board(&mut app), KeyCode::Enter), ScreenAction::None);
    assert!(!board(&mut app).confirm_pending());
    assert!(adopted_sessions(&app).is_empty());

    // Only `y` adopts.
    press(board(&mut app), KeyCode::Enter);
    let action = press(board(&mut app), KeyCode::Char('y'));
    handle_screen_action(&mut app, action);

    assert_eq!(app.tui_mode, TuiMode::Interaction);
    let adopted = adopted_sessions(&app);
    assert_eq!(adopted.len(), 1);
    assert_eq!(adopted[0].agent_session_id.as_deref(), Some(EXTERNAL_ID));
}

/// Adopting the same transcript twice from a stale board must not create a
/// second pool session bound to the same external id.
#[test]
fn fleet_adopt_refuses_an_id_already_in_the_pool() {
    let mut app = crate::tui::make_test_app("fleet-action-twice");
    let rows = build_rows(
        &[],
        "maestro",
        &[transcript(EXTERNAL_ID, REPO_CWD, "fix the parser", 86_400)],
    );
    open_board(&mut app, rows.clone());
    let first = press(board(&mut app), KeyCode::Enter);
    handle_screen_action(&mut app, first);
    assert_eq!(adopted_sessions(&app).len(), 1);

    // Back on the stale board: the row still reads External.
    app.tui_mode = TuiMode::FleetBoard;
    let second = press(board(&mut app), KeyCode::Enter);
    handle_screen_action(&mut app, second);

    assert_eq!(adopted_sessions(&app).len(), 1, "no second adoption");
    assert_ne!(app.tui_mode, TuiMode::Interaction, "refused, not routed");
    let status = board(&mut app)
        .status()
        .expect("refusal shows on the board");
    assert!(status.contains("already"), "status was: {status}");
}

/// Jump to a managed row reuses the live session. The pool must not gain a
/// session and the id must not change.
#[test]
fn fleet_jump_does_not_recreate_session() {
    let mut app = crate::tui::make_test_app("fleet-action-jump");
    let adopted_id = app
        .pool
        .create_adopted_session(
            EXTERNAL_ID,
            PathBuf::from(REPO_CWD),
            Some("fix the parser".to_string()),
            "opus".to_string(),
        )
        .expect("a real directory adopts")
        .session_id;
    let before = app.pool.all_sessions().len();

    let rows = build_rows(&app.pool.all_sessions(), "maestro", &[]);
    open_board(&mut app, rows);
    select(board(&mut app), |r| {
        matches!(r.source, FleetSource::Pool { .. })
    });
    let action = press(board(&mut app), KeyCode::Enter);
    assert_eq!(
        action,
        ScreenAction::OpenFleetRow {
            session_id: adopted_id
        }
    );
    handle_screen_action(&mut app, action);

    assert_eq!(app.tui_mode, TuiMode::Interaction);
    assert_eq!(app.pool.all_sessions().len(), before, "no new session");
    assert_eq!(adopted_sessions(&app).len(), 1);
}
