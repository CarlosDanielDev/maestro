//! Quick Actions (#1396). Since #1280 the Dashboard draws the Work list, so
//! the launcher panel is not on screen; the panel tests below are
//! component-only (`HomeScreen` drawn alone). Its letters still answer on
//! the Dashboard, tested through the real key path.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::{Terminal, backend::TestBackend};

use super::{HomeScreen, ProjectInfo, QUICK_ACTIONS, QuickActionDispatch};
use crate::tui::app::TuiMode;
use crate::tui::input_handler::handle_key;
use crate::tui::navigation::registry::{self, Chord};
use crate::tui::screens::Screen;

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

fn home_screen() -> HomeScreen {
    HomeScreen::new(
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: None,
        },
        vec![],
        vec![],
    )
}

/// The screen alone, so the panel can be shorter than the app's 80x24
/// size guard allows.
fn draw_screen(screen: &mut HomeScreen, w: u16, h: u16) -> Buffer {
    let theme = crate::tui::theme::Theme::default();
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| screen.draw(f, f.area(), &theme)).unwrap();
    t.backend().buffer().clone()
}

/// Only the Quick Actions panel, filling a `w`x`h` terminal.
fn draw_panel(screen: &HomeScreen, w: u16, h: u16) -> Buffer {
    let theme = crate::tui::theme::Theme::default();
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| screen.draw_quick_actions(f, f.area(), &theme))
        .unwrap();
    t.backend().buffer().clone()
}

/// Markers sit on the border, so every inner row is an action row and the
/// highlighted one is the selection: at the top, the bottom, and when only
/// one or two rows fit.
#[test]
fn quick_actions_selection_never_lands_on_a_marker_row() {
    let total = QUICK_ACTIONS.len();
    // Panel heights 3..=16: 1 to 14 inner rows (14 fits all 13).
    for h in 3u16..=16 {
        for (sel, (_, key, _)) in QUICK_ACTIONS.iter().enumerate() {
            let mut screen = home_screen();
            screen.selected_action = sel;
            let buf = draw_panel(&screen, 24, h);
            assert_eq!(panel_inner(&buf).height, h - 2, "h={h}: inner rows");
            let rows = panel_rows(&buf);
            let text = panel_with_border(&buf).join("\n");
            assert!(
                rows.iter()
                    .filter(|r| !r.trim().is_empty())
                    .all(|r| r.trim_start().starts_with('[')),
                "h={h} sel={sel}: a marker took a list row:\n{text}"
            );
            let hl = reversed_rows(&buf);
            assert_eq!(hl.len(), 1, "h={h} sel={sel}: one highlight:\n{text}");
            let want = format!("[{key}]");
            assert!(
                hl[0].contains(&want),
                "h={h} sel={sel}: {want} is highlighted:\n{text}"
            );
            let up = marker_count(&panel_with_border(&buf), '↑').unwrap_or(0);
            let down = marker_count(&panel_with_border(&buf), '↓').unwrap_or(0);
            assert_eq!(
                up + drawn_actions(&rows) + down,
                total,
                "h={h} sel={sel}: counts:\n{text}"
            );
        }
    }
}

/// A panel or screen with no room for any row draws without panicking,
/// and the hidden rows are still counted.
#[test]
fn quick_actions_tiny_panel_does_not_panic() {
    for h in 0u16..=6 {
        for w in [0u16, 1, 2, 10, 24, 80] {
            for sel in [0, 6, QUICK_ACTIONS.len() - 1] {
                let mut screen = home_screen();
                screen.selected_action = sel;
                draw_screen(&mut screen, w, h);
                draw_panel(&screen, w, h);
            }
        }
    }
    // Border only (no inner row): every row is hidden and counted.
    for sel in [0, 6, QUICK_ACTIONS.len() - 1] {
        let mut screen = home_screen();
        screen.selected_action = sel;
        let buf = draw_panel(&screen, 24, 2);
        let rows = panel_with_border(&buf);
        let up = marker_count(&rows, '↑').unwrap_or(0);
        let down = marker_count(&rows, '↓').unwrap_or(0);
        assert_eq!(up + down, QUICK_ACTIONS.len(), "sel={sel}: {rows:?}");
    }
}

/// Every launcher letter the globals leave opens its screen from the
/// Dashboard, through `handle_key`.
#[tokio::test]
async fn quick_actions_letters_work_on_the_dashboard() {
    for (label, key, dispatch) in QUICK_ACTIONS {
        let QuickActionDispatch::Push(want) = *dispatch else {
            continue;
        };
        if registry::is_global(Chord::key(KeyCode::Char(*key))) {
            continue;
        }
        let mut app = crate::tui::make_test_app("quick-actions-1280");
        app.screen_state.home_screen = Some(home_screen());
        app.tui_mode = TuiMode::Dashboard;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char(*key), KeyModifiers::NONE),
        )
        .await;
        assert_eq!(app.tui_mode, want, "[{key}] {label}");
    }
}
