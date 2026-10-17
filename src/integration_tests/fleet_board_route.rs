//! Fleet board → existing screens route (#1067).
//!
//! Builds the board's rows from a real `SessionPool` plus fixture
//! transcripts, then drives the board's key handling and the dispatcher's
//! `handle_screen_action` against the in-process `App`. No PTY, no
//! transcript writes; the adopted session uses a real directory so the
//! pool accepts its cwd.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crossterm::event::KeyCode;

use crate::session::discovery::DiscoveredSession;
use crate::tui::app::TuiMode;
use crate::tui::navigation::InputMode;
use crate::tui::screen_dispatch::handle_screen_action;
use crate::tui::screens::fleet_board::rows::build_rows;
use crate::tui::screens::fleet_board::{FleetBoardScreen, FleetRow, FleetSource};
use crate::tui::screens::{Screen, ScreenAction};

/// A real directory: `create_adopted_session` refuses cwds that are not dirs.
const REPO_CWD: &str = env!("CARGO_MANIFEST_DIR");
const ADOPTED_ID: &str = "ext-fleet-adopted-1";

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

#[test]
fn fleet_rows_route_to_existing_screens() {
    let mut app = crate::tui::make_test_app("fleet-board-route");
    app.pool
        .create_adopted_session(
            ADOPTED_ID,
            PathBuf::from(REPO_CWD),
            Some("fix the parser".to_string()),
            "opus".to_string(),
        )
        .expect("a real directory adopts");

    let pool = app.pool.all_sessions();
    let rows = build_rows(
        &pool,
        "maestro",
        &[
            transcript(ADOPTED_ID, REPO_CWD, "fix the parser", 60),
            transcript("ext-fleet-only-2", "/home/dev/api-service", "add auth", 90),
        ],
    );
    let mut board = FleetBoardScreen::new(rows, now());

    // A pool row (here the adopted one) opens its Interaction screen.
    select(&mut board, |r| matches!(r.source, FleetSource::Pool { .. }));
    let action = press(&mut board, KeyCode::Enter);
    assert!(
        matches!(action, ScreenAction::OpenFleetRow { .. }),
        "expected OpenFleetRow, got {action:?}"
    );
    handle_screen_action(&mut app, action);
    assert_eq!(app.tui_mode, TuiMode::Interaction);
    assert!(app.screen_state.interaction_screen.is_some());
}

#[test]
fn external_row_only_hints_at_adopt_and_stays_on_the_board() {
    let mut app = crate::tui::make_test_app("fleet-board-external");
    let rows = build_rows(
        &[],
        "maestro",
        &[transcript(
            "ext-fleet-only-3",
            "/home/dev/api-service",
            "add auth",
            90,
        )],
    );
    app.tui_mode = TuiMode::FleetBoard;
    app.screen_state.fleet_board_screen = Some(FleetBoardScreen::new(rows, now()));
    let board = app
        .screen_state
        .fleet_board_screen
        .as_mut()
        .expect("board was just set");

    let action = press(board, KeyCode::Enter);
    handle_screen_action(&mut app, action);

    assert_eq!(
        app.tui_mode,
        TuiMode::FleetBoard,
        "no navigation for external rows"
    );
    assert!(
        app.pool.all_sessions().is_empty(),
        "nothing adopted from the board"
    );
    let status = app
        .screen_state
        .fleet_board_screen
        .as_ref()
        .and_then(|b| b.status())
        .expect("the adopt hint must show on the board");
    assert!(status.contains("Adopt Session"), "status was: {status}");
}
