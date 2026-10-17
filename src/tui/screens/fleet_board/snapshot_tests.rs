use super::*;
use crate::tui::navigation::InputMode;
use crate::tui::screens::Screen;
use crate::tui::screens::test_helpers::key_event;
use crossterm::event::KeyCode;
use ratatui::{Terminal, backend::TestBackend};
use std::time::Duration;

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
}

fn managed(title: &str, repo: &str, age_secs: u64) -> FleetRow {
    FleetRow {
        kind: FleetKind::Managed,
        source: FleetSource::Pool {
            session_id: uuid::Uuid::nil(),
        },
        repo: repo.to_string(),
        title: title.to_string(),
        status: "running".to_string(),
        cwd: Some(PathBuf::from(format!("/home/dev/{repo}"))),
        last_active: Some(now() - Duration::from_secs(age_secs)),
    }
}

fn external(title: &str, repo: &str, age_secs: u64) -> FleetRow {
    FleetRow {
        kind: FleetKind::External,
        source: FleetSource::Transcript {
            agent_session_id: format!("ext-{repo}-{age_secs}"),
        },
        repo: repo.to_string(),
        title: title.to_string(),
        status: "external".to_string(),
        cwd: Some(PathBuf::from(format!("/home/dev/{repo}"))),
        last_active: Some(now() - Duration::from_secs(age_secs)),
    }
}

fn adopted(title: &str, repo: &str, age_secs: u64) -> FleetRow {
    FleetRow {
        kind: FleetKind::Adopted,
        source: FleetSource::Pool {
            session_id: uuid::Uuid::from_u128(7),
        },
        repo: repo.to_string(),
        title: title.to_string(),
        status: "completed".to_string(),
        cwd: Some(PathBuf::from(format!("/home/dev/{repo}"))),
        last_active: Some(now() - Duration::from_secs(age_secs)),
    }
}

/// One row of each kind, newest first in the default sort.
fn fixture() -> Vec<FleetRow> {
    vec![
        managed("wire the parser", "maestro", 30),
        adopted("fix parser tests", "maestro", 7_200),
        external("add auth endpoint", "api-service", 90_000),
    ]
}

fn render(screen: &mut FleetBoardScreen, width: u16, height: u16) -> String {
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

fn type_text(screen: &mut FleetBoardScreen, text: &str) {
    for c in text.chars() {
        screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Insert);
    }
}

#[test]
fn board_lists_managed_adopted_and_external_rows() {
    let mut screen = FleetBoardScreen::new(fixture(), now());
    insta::assert_snapshot!(render(&mut screen, 110, 10));
}

#[test]
fn board_filter_narrows_rows_by_repo_or_title() {
    let mut screen = FleetBoardScreen::new(fixture(), now());
    type_text(&mut screen, "api");
    insta::assert_snapshot!(render(&mut screen, 110, 10));
}

#[test]
fn board_empty_state_when_nothing_is_found() {
    let mut screen = FleetBoardScreen::new(Vec::new(), now());
    insta::assert_snapshot!(render(&mut screen, 110, 10));
}

#[test]
fn board_no_match_state_names_the_filter() {
    let mut screen = FleetBoardScreen::new(fixture(), now());
    type_text(&mut screen, "zzz");
    insta::assert_snapshot!(render(&mut screen, 110, 10));
}

/// Transcript-derived strings are untrusted: a title or cwd carrying ESC
/// or BEL must reach the buffer as spaces, never as control bytes.
#[test]
fn hostile_title_and_cwd_render_without_control_bytes() {
    let hostile_title = "\u{1b}]0;pwned\u{7}evil\u{1b}[2J";
    let mut row = external(hostile_title, "repo\u{1b}[31m", 60);
    row.cwd = Some(PathBuf::from("/home/dev/\u{7}bell"));
    row.status = "ext\u{1b}".to_string();
    let mut screen = FleetBoardScreen::new(vec![row], now());

    let rendered = render(&mut screen, 110, 8);

    assert!(!rendered.contains('\u{1b}'), "ESC reached the buffer");
    assert!(!rendered.contains('\u{7}'), "BEL reached the buffer");
    assert!(rendered.contains("evil"), "visible text must survive");
}
