use super::*;
use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::key_event;
use crossterm::event::KeyCode;
use ratatui::{Terminal, backend::TestBackend};
use std::time::Duration;

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

/// Newest first, as `scan_transcripts` returns them. One live row (30s old).
fn fixture() -> Vec<DiscoveredSession> {
    vec![
        session("live-1", "/home/dev/maestro", "still typing", 30),
        session("id-2", "/home/dev/api-service", "add auth endpoint", 7_200),
        session("id-3", "/home/dev/maestro", "docs pass", 90_000),
    ]
}

fn render(screen: &mut AdoptPickerScreen, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let theme = Theme::dark();
    terminal.draw(|f| screen.draw(f, f.area(), &theme)).unwrap();
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

fn type_text(screen: &mut AdoptPickerScreen, text: &str) {
    for c in text.chars() {
        screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Insert);
    }
}

#[test]
fn list_shows_repo_title_and_relative_time_newest_first() {
    let mut screen = AdoptPickerScreen::new(fixture(), now());
    insta::assert_snapshot!(render(&mut screen, 90, 10));
}

#[test]
fn live_row_carries_badge_and_confirm_modal_opens_on_enter() {
    let mut screen = AdoptPickerScreen::new(fixture(), now());
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    insta::assert_snapshot!(render(&mut screen, 90, 16));
}

#[test]
fn typing_filters_the_list_as_you_type() {
    let mut screen = AdoptPickerScreen::new(fixture(), now());
    type_text(&mut screen, "api");
    insta::assert_snapshot!(render(&mut screen, 90, 10));
}

#[test]
fn empty_machine_shows_first_run_empty_state() {
    let mut screen = AdoptPickerScreen::new(vec![], now());
    insta::assert_snapshot!(render(&mut screen, 90, 10));
}

#[test]
fn filter_with_no_match_shows_match_empty_state() {
    let mut screen = AdoptPickerScreen::new(fixture(), now());
    type_text(&mut screen, "zzz");
    insta::assert_snapshot!(render(&mut screen, 90, 10));
}
