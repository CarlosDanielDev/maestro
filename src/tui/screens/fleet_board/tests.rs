use super::*;
use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::key_event;
use crate::tui::screens::{Screen, ScreenAction};
use crossterm::event::KeyCode;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_000_000)
}

fn pool_row(title: &str, age_secs: u64) -> FleetRow {
    FleetRow {
        kind: FleetKind::Managed,
        source: FleetSource::Pool {
            session_id: Uuid::new_v4(),
        },
        repo: "maestro".to_string(),
        title: title.to_string(),
        status: "running".to_string(),
        cwd: None,
        last_active: Some(now() - Duration::from_secs(age_secs)),
    }
}

fn external_row(title: &str, age_secs: u64) -> FleetRow {
    FleetRow {
        kind: FleetKind::External,
        source: FleetSource::Transcript {
            agent_session_id: format!("ext-{title}"),
        },
        repo: "api-service".to_string(),
        title: title.to_string(),
        status: "external".to_string(),
        cwd: Some(PathBuf::from("/home/dev/api-service")),
        last_active: Some(now() - Duration::from_secs(age_secs)),
    }
}

fn press(screen: &mut FleetBoardScreen, code: KeyCode) -> ScreenAction {
    screen.handle_input(&key_event(code), InputMode::Insert)
}

fn type_text(screen: &mut FleetBoardScreen, text: &str) {
    for c in text.chars() {
        press(screen, KeyCode::Char(c));
    }
}

fn visible_titles(screen: &FleetBoardScreen) -> Vec<String> {
    screen
        .filtered_rows()
        .iter()
        .map(|r| r.title.clone())
        .collect()
}

#[test]
fn tab_toggles_between_activity_and_stale_first_order() {
    let mut screen = FleetBoardScreen::new(vec![pool_row("old", 500), pool_row("fresh", 5)], now());
    assert_eq!(visible_titles(&screen), ["fresh", "old"]);

    press(&mut screen, KeyCode::Tab);
    assert_eq!(screen.sort(), FleetSort::StaleFirst);
    assert_eq!(visible_titles(&screen), ["old", "fresh"]);

    press(&mut screen, KeyCode::Tab);
    assert_eq!(visible_titles(&screen), ["fresh", "old"]);
}

#[test]
fn enter_on_pool_row_asks_to_open_that_session() {
    let row = pool_row("managed work", 10);
    let FleetSource::Pool { session_id } = row.source else {
        unreachable!("pool_row builds a pool source");
    };
    let mut screen = FleetBoardScreen::new(vec![row], now());

    let action = press(&mut screen, KeyCode::Enter);

    assert_eq!(action, ScreenAction::OpenFleetRow { session_id });
}

#[test]
fn enter_on_external_row_opens_nothing_and_shows_adopt_hint_on_screen() {
    let mut screen = FleetBoardScreen::new(vec![external_row("ext work", 10)], now());

    let action = press(&mut screen, KeyCode::Enter);

    assert_eq!(action, ScreenAction::None);
    let status = screen.status().expect("hint must show on the board");
    assert!(status.contains("Adopt Session"), "got: {status}");
}

#[test]
fn typing_filters_and_esc_clears_the_filter_before_leaving() {
    let mut screen = FleetBoardScreen::new(
        vec![pool_row("docs pass", 10), external_row("add auth", 20)],
        now(),
    );

    type_text(&mut screen, "auth");
    assert_eq!(visible_titles(&screen), ["add auth"]);

    assert_eq!(press(&mut screen, KeyCode::Esc), ScreenAction::None);
    assert_eq!(
        visible_titles(&screen).len(),
        2,
        "first Esc clears the filter"
    );

    assert_eq!(press(&mut screen, KeyCode::Esc), ScreenAction::Pop);
}

#[test]
fn next_key_clears_a_stale_status_line() {
    let mut screen = FleetBoardScreen::new(vec![external_row("ext work", 10)], now());
    press(&mut screen, KeyCode::Enter);
    assert!(screen.status().is_some());

    press(&mut screen, KeyCode::Down);

    assert!(screen.status().is_none());
}
