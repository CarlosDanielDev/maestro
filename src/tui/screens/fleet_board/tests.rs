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

/// A stale external row asks the dispatcher for the picker's adopt action,
/// with the same fields the picker sends.
#[test]
fn enter_on_stale_external_row_asks_to_adopt_it() {
    let row = external_row("ext work", 86_400);
    let FleetSource::Transcript { agent_session_id } = row.source.clone() else {
        unreachable!("external_row builds a transcript source");
    };
    let mut screen = FleetBoardScreen::new(vec![row], now());

    let action = press(&mut screen, KeyCode::Enter);

    assert_eq!(
        action,
        ScreenAction::AdoptSession {
            agent_session_id,
            cwd: PathBuf::from("/home/dev/api-service"),
            title: Some("ext work".to_string()),
        }
    );
}

/// A live external row must not adopt on Enter: it opens the confirm modal.
#[test]
fn enter_on_live_external_row_opens_the_confirm_and_adopts_nothing() {
    let mut screen = FleetBoardScreen::new(vec![external_row("ext work", 10)], now());

    assert_eq!(press(&mut screen, KeyCode::Enter), ScreenAction::None);
    assert!(screen.confirm_pending());
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
    screen.set_status("adoption refused".to_string());
    assert!(screen.status().is_some());

    press(&mut screen, KeyCode::Down);

    assert!(screen.status().is_none());
}

/// The board's own confirm call site: Ctrl+y and Alt+y on a live row cancel.
#[test]
fn ctrl_and_alt_y_on_the_live_confirm_do_not_adopt() {
    use crossterm::event::{Event, KeyEvent, KeyModifiers};

    for mods in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        let mut screen = FleetBoardScreen::new(vec![external_row("ext work", 10)], now());
        press(&mut screen, KeyCode::Enter);
        assert!(screen.confirm_pending(), "Enter opens the confirm first");

        let chord = Event::Key(KeyEvent::new(KeyCode::Char('y'), mods));
        let action = screen.handle_input(&chord, InputMode::Insert);

        assert_eq!(action, ScreenAction::None, "{mods:?}+y must not adopt");
        assert!(!screen.confirm_pending(), "{mods:?}+y closes the confirm");
    }
}

/// Ctrl and Alt chords are commands, not filter text. Shift still types.
#[test]
fn ctrl_and_alt_letters_do_not_enter_the_filter() {
    use crossterm::event::{Event, KeyEvent, KeyModifiers};

    let mut screen = FleetBoardScreen::new(vec![pool_row("docs pass", 10)], now());
    for mods in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        let chord = Event::Key(KeyEvent::new(KeyCode::Char('x'), mods));
        screen.handle_input(&chord, InputMode::Insert);
    }
    assert!(
        screen.filter_text.is_empty(),
        "got {:?}",
        screen.filter_text
    );

    let shifted = Event::Key(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT));
    screen.handle_input(&shifted, InputMode::Insert);
    assert_eq!(screen.filter_text, "D");
}
