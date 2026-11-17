//! The shell rows of `ui::draw` (#1258): TabBar on the seven top-level
//! screens, CrumbBar inside flows, and one footer of key hints that always
//! ends with the quit hint (`ds::key_hints::QUIT`).

use insta::assert_snapshot;
use ratatui::style::Modifier;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::app::{App, TuiMode};
use crate::tui::screens::IssueWizardScreen;
use crate::tui::screens::home::{HomeScreen, ProjectInfo};
use crate::tui::{make_test_app, ui};

fn shell_app(mode: TuiMode) -> App {
    let mut app = make_test_app("shell-bars");
    app.show_mascot = false;
    app.show_activity_log = false;
    app.spinner_tick = 0;
    app.gh_auth_ok = true;
    app.bypass_active = false;
    app.no_splash = true;
    app.status_bar_marquee = crate::tui::marquee::MarqueeState::new();
    app.status_bar_marquee_fingerprint = 0;
    app.resource_monitor = Box::new(crate::system::monitor::MockResourceMonitor::new(
        22 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
    ));
    app.screen_state.home_screen = Some(HomeScreen::new(
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: Some("carlos".to_string()),
        },
        vec![],
        vec![],
    ));
    app.screen_state.issue_wizard_screen = Some(IssueWizardScreen::new());
    app.tui_mode = mode;
    app
}

fn draw(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    t
}

/// The pinned quit hint as drawn, read from the const (#1260 changes it).
fn quit_hint() -> String {
    let (key, label) = crate::tui::ds::key_hints::QUIT;
    format!("{key} {label}")
}

fn row_text(t: &Terminal<TestBackend>, y: u16) -> String {
    let buf = t.backend().buffer();
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

fn find_row(t: &Terminal<TestBackend>, needle: &str) -> Option<u16> {
    (0..t.backend().buffer().area.height).find(|y| row_text(t, *y).contains(needle))
}

#[test]
fn tab_bar_active_dashboard() {
    let mut app = shell_app(TuiMode::Dashboard);
    let t = draw(&mut app, 80, 24);
    let y = find_row(&t, "2 Dashboard").expect("tab bar row");
    let row = row_text(&t, y);
    for tab in [
        "1 Home",
        "3 Issues",
        "4 Milestones",
        "5 Roadmap",
        "6 Stats",
        "7 PRD",
    ] {
        assert!(row.contains(tab), "{tab} missing from {row:?}");
    }
    let buf = t.backend().buffer();
    let x = row.find("2 Dashboard").unwrap();
    let x = row[..x].chars().count() as u16;
    assert!(
        buf[(x, y)].modifier.contains(Modifier::REVERSED),
        "active tab is reversed"
    );
    assert!(
        !buf[(x - 9, y)].modifier.contains(Modifier::REVERSED),
        "Home tab is not reversed"
    );
    assert_snapshot!(t.backend());
}

#[test]
fn issue_wizard_shows_crumb_bar_not_tab_bar() {
    let mut app = shell_app(TuiMode::IssueWizard);
    app.nav_stack.push(TuiMode::Landing);
    let t = draw(&mut app, 80, 24);
    let out = format!("{}", t.backend());
    assert!(out.contains("Home › New issue"), "crumb bar:\n{out}");
    assert!(!out.contains("2 Dashboard"), "no tab bar in a flow:\n{out}");
    assert_snapshot!(t.backend());
}

#[test]
fn footer_pins_quit() {
    for mode in [TuiMode::Dashboard, TuiMode::IssueWizard, TuiMode::Overview] {
        for w in [80u16, 100, 150] {
            let mut app = shell_app(mode);
            let t = draw(&mut app, w, 24);
            let last = row_text(&t, 23);
            assert!(
                last.trim_end().ends_with(&quit_hint()),
                "{mode:?} at {w} cols: footer {last:?}"
            );
        }
    }
}

#[test]
fn quit_dialog_footer_has_no_quit_pin() {
    let mut app = shell_app(TuiMode::ConfirmExit);
    app.nav_stack.push(TuiMode::Dashboard);
    let t = draw(&mut app, 100, 24);
    assert!(!row_text(&t, 23).contains(&quit_hint()));
}
