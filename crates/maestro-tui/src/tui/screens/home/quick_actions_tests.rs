//! Quick Actions scroll (#1396): the Dashboard panel holds 13 rows but only
//! 6 fit at 80x24 (8 at 80x27). These tests drive the full app (`2`, then
//! `j`) and read what `ui::draw` puts on the screen.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::{Terminal, backend::TestBackend};

use super::{HomeScreen, ProjectInfo, QUICK_ACTIONS};
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::{handle_key, handle_key_under_size_guard};
use crate::tui::navigation::InputMode;
use crate::tui::screens::{LandingScreen, Screen};
use crate::tui::{make_test_app, ui};

fn dashboard_app() -> App {
    let mut app = make_test_app("quick-actions-1396");
    app.show_mascot = false;
    app.spinner_tick = 0;
    app.gh_auth_ok = true;
    app.bypass_active = false;
    app.no_splash = true;
    app.resource_monitor = Box::new(crate::system::monitor::MockResourceMonitor::new(
        22 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
    ));
    app.screen_state.home_screen = Some(HomeScreen::new(
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: None,
        },
        vec![],
        vec![],
    ));
    app.screen_state.landing_screen = Some(LandingScreen::new());
    app.tui_mode = TuiMode::Landing;
    app
}

async fn press(app: &mut App, code: KeyCode, w: u16, h: u16) {
    let key = KeyEvent::new(code, KeyModifiers::NONE);
    if handle_key_under_size_guard(app, key, Rect::new(0, 0, w, h))
        .await
        .is_none()
    {
        handle_key(app, key).await;
    }
}

/// `2` opens the Dashboard, then `j` is pressed `downs` times.
async fn dashboard_after(downs: usize, w: u16, h: u16) -> App {
    let mut app = dashboard_app();
    press(&mut app, KeyCode::Char('2'), w, h).await;
    assert_eq!(app.tui_mode, TuiMode::Dashboard, "`2` must open Dashboard");
    for _ in 0..downs {
        press(&mut app, KeyCode::Char('j'), w, h).await;
    }
    app
}

fn draw(app: &mut App, w: u16, h: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    t.backend().buffer().clone()
}

fn row_text(buf: &Buffer, y: u16, x0: u16, x1: u16) -> String {
    (x0..x1).map(|x| buf[(x, y)].symbol()).collect()
}

/// Inner rect of the Quick Actions panel: found by its title row, its right
/// corner, and the bottom corner below the left edge.
fn panel_inner(buf: &Buffer) -> Rect {
    let w = buf.area.width;
    let title_y = (0..buf.area.height)
        .find(|&y| row_text(buf, y, 0, w).contains("Quick Actions"))
        .expect("Quick Actions panel is drawn");
    let right = (1..w)
        .find(|&x| matches!(buf[(x, title_y)].symbol(), "┓" | "┐"))
        .expect("panel right corner");
    let bottom = (title_y + 1..buf.area.height)
        .find(|&y| matches!(buf[(0, y)].symbol(), "┗" | "└"))
        .expect("panel bottom corner");
    Rect::new(1, title_y + 1, right - 1, bottom - title_y - 1)
}

fn panel_rows(buf: &Buffer) -> Vec<String> {
    let r = panel_inner(buf);
    (r.y..r.bottom())
        .map(|y| row_text(buf, y, r.x, r.right()))
        .collect()
}

fn reversed_rows(buf: &Buffer) -> Vec<String> {
    let r = panel_inner(buf);
    (r.y..r.bottom())
        .filter(|&y| (r.x..r.right()).any(|x| buf[(x, y)].modifier.contains(Modifier::REVERSED)))
        .map(|y| row_text(buf, y, r.x, r.right()))
        .collect()
}

/// Number of Quick Action rows drawn (`[x]` prefix).
fn drawn_actions(rows: &[String]) -> usize {
    rows.iter()
        .filter(|r| r.trim_start().starts_with('['))
        .count()
}

/// Inner rows plus the bottom border, where the overflow markers sit.
fn panel_with_border(buf: &Buffer) -> Vec<String> {
    let r = panel_inner(buf);
    let mut rows = panel_rows(buf);
    rows.push(row_text(buf, r.bottom(), r.x - 1, r.right() + 1));
    rows
}

/// `N` from a `<arrow> N more` marker anywhere in `rows`.
fn marker_count(rows: &[String], arrow: char) -> Option<usize> {
    rows.iter().find_map(|r| {
        let (_, rest) = r.split_once(arrow)?;
        let mut words = rest.split_whitespace();
        let n = words.next()?.parse().ok()?;
        (words.next()? == "more").then_some(n)
    })
}

#[tokio::test]
async fn quick_actions_keeps_selection_visible_when_list_overflows() {
    for h in [24u16, 27] {
        let mut app = dashboard_after(12, 80, h).await;
        let buf = draw(&mut app, 80, h);
        let rows = panel_rows(&buf);
        let hl = reversed_rows(&buf);
        assert_eq!(
            hl.len(),
            1,
            "80x{h}: one highlighted row:\n{}",
            rows.join("\n")
        );
        assert!(
            hl[0].contains("[q]  Quit"),
            "80x{h}: highlight is Quit: {hl:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("[q]  Quit")),
            "80x{h}: Quit is drawn:\n{}",
            rows.join("\n")
        );
    }
}

#[tokio::test]
async fn quick_actions_shows_overflow_marker() {
    let total = QUICK_ACTIONS.len();
    for h in [24u16, 27] {
        for downs in 0..total {
            let mut app = dashboard_after(downs, 80, h).await;
            let rows = panel_with_border(&draw(&mut app, 80, h));
            let shown = drawn_actions(&rows);
            let up = marker_count(&rows, '↑').unwrap_or(0);
            let down = marker_count(&rows, '↓').unwrap_or(0);
            let text = rows.join("\n");
            assert!(
                shown < total,
                "80x{h} j×{downs}: list must overflow:\n{text}"
            );
            assert!(up + down > 0, "80x{h} j×{downs}: a marker shows:\n{text}");
            assert_eq!(
                up + shown + down,
                total,
                "80x{h} j×{downs}: markers count the hidden rows:\n{text}"
            );
        }
        let top = panel_with_border(&draw(&mut dashboard_after(0, 80, h).await, 80, h));
        assert_eq!(marker_count(&top, '↑'), None, "80x{h}: no ↑ at the top");
        let end = panel_with_border(&draw(&mut dashboard_after(total - 1, 80, h).await, 80, h));
        assert_eq!(marker_count(&end, '↓'), None, "80x{h}: no ↓ at the end");
    }
}

#[tokio::test]
async fn quick_actions_enter_runs_only_the_drawn_selection() {
    for h in [24u16, 27] {
        for downs in 0..QUICK_ACTIONS.len() {
            let mut app = dashboard_after(downs, 80, h).await;
            let hl = reversed_rows(&draw(&mut app, 80, h));
            assert_eq!(hl.len(), 1, "80x{h} j×{downs}: one highlighted row: {hl:?}");
            let key = hl[0]
                .trim_start()
                .strip_prefix('[')
                .and_then(|s| s.chars().next())
                .expect("highlighted row starts with [key]");

            let home = app.screen_state.home_screen.as_mut().unwrap();
            let by_enter = home.handle_input(
                &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                InputMode::Normal,
            );
            let by_letter = home.handle_input(
                &Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                InputMode::Normal,
            );
            assert_eq!(
                by_enter, by_letter,
                "80x{h} j×{downs}: Enter must run the drawn row [{key}]"
            );
        }
    }
}

#[tokio::test]
async fn quick_actions_scrolled_to_end_80x24() {
    let mut app = dashboard_after(QUICK_ACTIONS.len() - 1, 80, 24).await;
    let rows = panel_with_border(&draw(&mut app, 80, 24));
    let text = rows.join("\n");
    for key in ["[S]", "[Q]", "[t]", "[u]"] {
        assert!(text.contains(key), "{key} is drawn:\n{text}");
    }
    insta::assert_snapshot!(text);
}
