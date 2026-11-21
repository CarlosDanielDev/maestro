//! #1241 — while gh is signed out, the Dashboard's log row says
//! `⊘ gh auth expired`, what it locks, and the fix. Drawn through the
//! full `ui::draw` at 80x24 and 80x27.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::app::{App, TuiMode};
use crate::tui::screens::home::{HomeScreen, ProjectInfo};
use crate::tui::{make_test_app, ui};

fn signed_out_dashboard() -> App {
    let mut app = make_test_app("gh-auth-expired-snapshot");
    let info = ProjectInfo {
        repo: "owner/repo".to_string(),
        branch: "main".to_string(),
        username: Some("carlos".to_string()),
    };
    app.screen_state.home_screen = Some(HomeScreen::new(info, vec![], vec![]));
    app.tui_mode = TuiMode::Dashboard;
    app.gh_auth_ok = false;
    app
}

fn draw(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    t
}

fn text(t: &Terminal<TestBackend>) -> String {
    let buf = t.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_expired_line(t: &Terminal<TestBackend>) {
    let text = text(t);
    assert!(text.contains("⊘ gh auth expired"), "header word:\n{text}");
    assert!(text.contains("PR create paused"), "what is locked:\n{text}");
    assert!(
        text.contains("gh auth login, then ⏎ ⏎ on Dashboard"),
        "the fix:\n{text}"
    );
}

#[test]
fn dashboard_gh_auth_expired() {
    let mut app = signed_out_dashboard();
    let t = draw(&mut app, 80, 24);
    assert_expired_line(&t);
    assert_snapshot!("dashboard_gh_auth_expired", t.backend());
}

#[test]
fn dashboard_gh_auth_expired_80x27() {
    let mut app = signed_out_dashboard();
    let t = draw(&mut app, 80, 27);
    assert_expired_line(&t);
    assert_snapshot!("dashboard_gh_auth_expired_80x27", t.backend());
}

#[test]
fn dashboard_gh_auth_line_gone_once_signed_in() {
    let mut app = signed_out_dashboard();
    app.gh_auth_ok = true;
    let t = draw(&mut app, 80, 24);
    assert!(!text(&t).contains("gh auth expired"), "{}", text(&t));
}
