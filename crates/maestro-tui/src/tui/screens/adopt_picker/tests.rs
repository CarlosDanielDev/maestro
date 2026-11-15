use super::*;
use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::key_event;
use crossterm::event::KeyCode;
use std::time::{Duration, SystemTime};

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
}

fn session(id: &str, cwd: &str, title: &str, age_secs: u64) -> DiscoveredSession {
    DiscoveredSession {
        session_id: id.to_string(),
        cwd: PathBuf::from(cwd),
        title: title.to_string(),
        last_active: now() - Duration::from_secs(age_secs),
        skipped_lines: 0,
    }
}

/// Stale rows: well outside the quiet period, adopt without a prompt.
fn stale_fixtures() -> Vec<DiscoveredSession> {
    vec![
        session("id-1", "/repo/maestro", "fix the parser", 86_400),
        session("id-2", "/work/api-service", "add auth endpoint", 7_200),
        session("id-3", "/repo/maestro", "docs pass", 300 * 1_000),
    ]
}

fn screen_with(sessions: Vec<DiscoveredSession>) -> AdoptPickerScreen {
    AdoptPickerScreen::new(sessions, now())
}

fn type_text(screen: &mut AdoptPickerScreen, text: &str) {
    for c in text.chars() {
        screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Insert);
    }
}

#[test]
fn new_starts_first_row_selected() {
    let screen = screen_with(stale_fixtures());
    assert_eq!(
        screen.selected_session().map(|s| s.session_id.as_str()),
        Some("id-1")
    );
}

#[test]
fn empty_filter_shows_every_session() {
    let screen = screen_with(stale_fixtures());
    assert_eq!(screen.filtered_sessions().len(), 3);
}

#[test]
fn typing_filters_by_title_case_insensitive() {
    let mut screen = screen_with(stale_fixtures());
    type_text(&mut screen, "AUTH");
    let filtered = screen.filtered_sessions();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].session_id, "id-2");
}

#[test]
fn typing_filters_by_repo_name_from_cwd() {
    let mut screen = screen_with(stale_fixtures());
    type_text(&mut screen, "api");
    assert_eq!(screen.filtered_sessions().len(), 1);
    assert_eq!(screen.filtered_sessions()[0].session_id, "id-2");
}

#[test]
fn no_match_yields_empty_result_not_panic() {
    let mut screen = screen_with(stale_fixtures());
    type_text(&mut screen, "zzznomatch");
    assert!(screen.filtered_sessions().is_empty());
    assert!(screen.selected_session().is_none());
}

#[test]
fn selection_clamps_when_filter_shrinks_list() {
    let mut screen = screen_with(stale_fixtures());
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    type_text(&mut screen, "parser");
    assert_eq!(
        screen.selected_session().map(|s| s.session_id.as_str()),
        Some("id-1")
    );
}

#[test]
fn backspace_removes_last_filter_char() {
    let mut screen = screen_with(stale_fixtures());
    type_text(&mut screen, "par");
    screen.handle_input(&key_event(KeyCode::Backspace), InputMode::Insert);
    assert_eq!(screen.filter_text, "pa");
}

#[test]
fn esc_clears_filter_first_then_pops() {
    let mut screen = screen_with(stale_fixtures());
    type_text(&mut screen, "par");
    let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert!(screen.filter_text.is_empty());
    let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
    assert_eq!(action, ScreenAction::Pop);
}

#[test]
fn enter_on_stale_row_adopts_immediately() {
    let mut screen = screen_with(stale_fixtures());
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    match action {
        ScreenAction::AdoptSession {
            agent_session_id,
            cwd,
            title,
        } => {
            assert_eq!(agent_session_id, "id-1");
            assert_eq!(cwd, PathBuf::from("/repo/maestro"));
            assert_eq!(title.as_deref(), Some("fix the parser"));
        }
        other => panic!("expected AdoptSession, got {other:?}"),
    }
}

#[test]
fn adopt_turns_empty_title_into_none() {
    let mut sessions = stale_fixtures();
    sessions[0].title = String::new();
    let mut screen = screen_with(sessions);
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    match action {
        ScreenAction::AdoptSession { title, .. } => assert_eq!(title, None),
        other => panic!("expected AdoptSession, got {other:?}"),
    }
}

#[test]
fn enter_on_empty_list_is_noop() {
    let mut screen = screen_with(vec![]);
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn is_live_only_inside_the_quiet_period() {
    let screen = screen_with(vec![]);
    let fresh = session("f", "/r", "t", 599);
    let stale = session("s", "/r", "t", 601);
    let skewed = DiscoveredSession {
        last_active: now() + Duration::from_secs(30), // clock skew = fresh
        ..session("k", "/r", "t", 0)
    };
    assert!(screen.is_live(&fresh));
    assert!(!screen.is_live(&stale));
    assert!(screen.is_live(&skewed));
}

// ---- live rows confirm before adopting (#1054) ----

fn live_then_stale() -> Vec<DiscoveredSession> {
    vec![
        session("live-1", "/repo/hot", "still typing", 30),
        session("stale-1", "/repo/cold", "long done", 86_400),
    ]
}

#[test]
fn enter_on_live_row_asks_instead_of_adopting() {
    let mut screen = screen_with(live_then_stale());
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(action, ScreenAction::None, "live row must not adopt yet");
    assert!(screen.confirm_pending(), "confirm modal must be open");
}

#[test]
fn confirm_yes_adopts_the_live_row() {
    let mut screen = screen_with(live_then_stale());
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    let action = screen.handle_input(&key_event(KeyCode::Char('y')), InputMode::Insert);
    match action {
        ScreenAction::AdoptSession {
            agent_session_id, ..
        } => {
            assert_eq!(agent_session_id, "live-1")
        }
        other => panic!("expected AdoptSession after confirm, got {other:?}"),
    }
    assert!(!screen.confirm_pending());
}

#[test]
fn confirm_enter_cancels() {
    let mut screen = screen_with(live_then_stale());
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    assert_eq!(action, ScreenAction::None, "Enter must not adopt");
    assert!(!screen.confirm_pending(), "Enter closes the modal");
}

#[test]
fn double_enter_never_adopts_a_live_row() {
    let mut screen = screen_with(live_then_stale());
    let mut actions = Vec::new();
    for _ in 0..3 {
        actions.push(screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert));
    }
    assert!(
        actions.iter().all(|a| *a == ScreenAction::None),
        "no Enter sequence may adopt a live row: {actions:?}"
    );
    assert!(
        screen.confirm_pending(),
        "the third Enter reopens the modal"
    );
}

#[test]
fn uppercase_y_adopts_the_live_row() {
    let mut screen = screen_with(live_then_stale());
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    let action = screen.handle_input(&key_event(KeyCode::Char('Y')), InputMode::Insert);
    assert!(matches!(action, ScreenAction::AdoptSession { .. }));
}

/// A chord that ends in `y` is a command, not a yes: Ctrl+y and Alt+y must
/// cancel the modal without adopting a live row.
#[test]
fn ctrl_and_alt_y_do_not_adopt_a_live_row() {
    use crossterm::event::{Event, KeyEvent, KeyModifiers};
    for mods in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        let mut screen = screen_with(live_then_stale());
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        let chord = Event::Key(KeyEvent::new(KeyCode::Char('y'), mods));
        let action = screen.handle_input(&chord, InputMode::Insert);
        assert_eq!(action, ScreenAction::None, "{mods:?}+y must not adopt");
        assert!(!screen.confirm_pending(), "{mods:?}+y closes the modal");
    }
}

/// Shift is part of typing a capital, so Shift+y still adopts.
#[test]
fn shift_y_adopts_the_live_row() {
    use crossterm::event::{Event, KeyEvent, KeyModifiers};
    let mut screen = screen_with(live_then_stale());
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    let shifted = Event::Key(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT));
    let action = screen.handle_input(&shifted, InputMode::Insert);
    assert!(matches!(action, ScreenAction::AdoptSession { .. }));
}

#[test]
fn confirm_cancel_stays_on_the_picker() {
    for cancel in [KeyCode::Char('n'), KeyCode::Esc] {
        let mut screen = screen_with(live_then_stale());
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        let action = screen.handle_input(&key_event(cancel), InputMode::Insert);
        assert_eq!(action, ScreenAction::None);
        assert!(!screen.confirm_pending());
        // Still on the list: Enter re-opens the confirm instead of adopting.
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert!(
            screen.confirm_pending(),
            "must stay on the picker after cancel"
        );
    }
}

#[test]
fn other_keys_cancel_the_confirm_modal() {
    let mut screen = screen_with(live_then_stale());
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    let action = screen.handle_input(&key_event(KeyCode::Char('x')), InputMode::Insert);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(
        screen.filter_text, "",
        "a cancelling key must not reach the filter"
    );
    assert!(!screen.confirm_pending());
}

// ---- navigation ----

#[test]
fn down_moves_and_stops_at_last_row() {
    let mut screen = screen_with(stale_fixtures());
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    assert_eq!(
        screen.selected_session().map(|s| s.session_id.as_str()),
        Some("id-2")
    );
    for _ in 0..10 {
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    }
    assert_eq!(
        screen.selected_session().map(|s| s.session_id.as_str()),
        Some("id-3")
    );
}

#[test]
fn up_does_not_underflow() {
    let mut screen = screen_with(stale_fixtures());
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Insert);
    assert_eq!(
        screen.selected_session().map(|s| s.session_id.as_str()),
        Some("id-1")
    );
}

#[test]
fn desired_input_mode_is_insert_while_typing_filter() {
    let screen = screen_with(stale_fixtures());
    assert_eq!(screen.desired_input_mode(), Some(InputMode::Insert));
}

#[test]
fn status_line_survives_until_the_next_key() {
    let mut screen = screen_with(stale_fixtures());
    screen.set_status("adoption refused: nope".to_string());
    assert_eq!(screen.status(), Some("adoption refused: nope"));
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Insert);
    assert_eq!(screen.status(), None);
}
