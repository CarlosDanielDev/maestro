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

/// Transcript title and cwd are untrusted: ANSI/OSC sequences must never
/// reach the terminal (#1043 class).
#[test]
fn hostile_title_and_cwd_render_without_control_bytes() {
    let rows = vec![DiscoveredSession {
        session_id: "h-1".to_string(),
        cwd: PathBuf::from("/home/dev/\x1b]0;pwn\x07repo"),
        title: "\x1b]0;pwn\x07ok \x1b[31mred".to_string(),
        last_active: now() - Duration::from_secs(7_200),
        skipped_lines: 0,
    }];
    let mut screen = AdoptPickerScreen::new(rows, now());
    let out = render(&mut screen, 90, 10);
    assert!(!out.contains('\x1b'), "ESC must not reach the terminal");
    assert!(!out.contains('\x07'), "BEL must not reach the terminal");
    assert!(out.contains("ok"), "printable title text stays visible");
    assert!(
        out.contains("pwn"),
        "printable part of the OSC payload stays"
    );
}

#[test]
fn selected_row_shows_the_full_cwd() {
    let rows = vec![session(
        "id-1",
        "/home/dev/projects/very/deep/maestro-app",
        "docs",
        60,
    )];
    let mut screen = AdoptPickerScreen::new(rows, now());
    let out = render(&mut screen, 90, 10);
    assert!(out.contains("/home/dev/projects/very/deep/maestro-app"));
}

#[test]
fn confirm_modal_shows_the_full_cwd_being_adopted() {
    let rows = vec![session("live-1", "/home/dev/maestro-app", "typing", 30)];
    let mut screen = AdoptPickerScreen::new(rows, now());
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
    let out = render(&mut screen, 90, 16);
    assert!(out.contains("/home/dev/maestro-app"));
}

#[test]
fn refused_adoption_status_is_drawn_on_the_picker() {
    let mut screen = AdoptPickerScreen::new(fixture(), now());
    screen.set_status("adoption refused: adopted cwd is not a directory".to_string());
    let out = render(&mut screen, 90, 10);
    assert!(out.contains("adoption refused: adopted cwd is not a directory"));
}
