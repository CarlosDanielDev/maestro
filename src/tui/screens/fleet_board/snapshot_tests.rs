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

/// The empty-state headline echoes the filter, so a control byte typed into
/// the filter must not reach the buffer.
#[test]
fn empty_state_echoes_filter_without_control_bytes() {
    let mut screen = FleetBoardScreen::new(fixture(), now());
    screen.handle_input(&key_event(KeyCode::Char('\u{1b}')), InputMode::Insert);

    let rendered = render(&mut screen, 110, 10);

    assert!(
        rendered.contains("No sessions match"),
        "empty state must show"
    );
    assert!(!rendered.contains('\u{1b}'), "ESC reached the buffer");
}

/// The live confirm names the directory, and a hostile title or cwd in it
/// must reach the buffer as spaces, like every other board sink.
#[test]
fn live_confirm_renders_hostile_cwd_without_control_bytes() {
    let mut row = external("evil\u{1b}]0;x\u{7}", "repo", 10);
    row.cwd = Some(PathBuf::from("/home/dev/\u{1b}[2J"));
    let mut screen = FleetBoardScreen::new(vec![row], now());
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);

    let rendered = render(&mut screen, 110, 24);

    assert!(
        rendered.contains("Session may be live"),
        "confirm must show"
    );
    assert!(rendered.contains("Directory:"), "cwd line must show");
    assert!(!rendered.contains('\u{1b}'), "ESC reached the buffer");
    assert!(!rendered.contains('\u{7}'), "BEL reached the buffer");
}

/// The footer names every board action: adopt or open, sort, filter, back.
#[test]
fn fleet_board_footer_keymap() {
    let mut screen = FleetBoardScreen::new(fixture(), now());

    let rendered = render(&mut screen, 110, 10);
    let footer = rendered.lines().last().expect("board has rows");

    for token in ["Navigate", "Filter", "Sort", "Open/Adopt", "Back"] {
        assert!(footer.contains(token), "footer lacks {token}: {footer}");
    }
    insta::assert_snapshot!(footer.trim_end());
}
