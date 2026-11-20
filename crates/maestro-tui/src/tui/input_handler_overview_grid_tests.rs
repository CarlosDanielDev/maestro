//! #1384: Overview arrow and page keys move the grid the Overview really
//! draws, so the highlighted panel is always `panel_view.selected`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

use super::handle_key;
use crate::session::types::Session;
use crate::tui::app::{App, TuiMode};
use crate::tui::panels::GridLayout;

/// Issue numbers start here so no number is a prefix of another.
const FIRST_ISSUE: u64 = 101;

fn overview_app(sessions: usize) -> App {
    let mut app = crate::tui::make_test_app("overview-grid-keys");
    app.show_mascot = false;
    app.show_activity_log = false;
    app.no_splash = true;
    for i in 0..sessions {
        let issue = FIRST_ISSUE + i as u64;
        app.pool.enqueue(Session::new(
            format!("task {issue}"),
            "claude-opus-4-5".into(),
            "orchestrator".into(),
            Some(issue),
            None,
        ));
    }
    app.tui_mode = TuiMode::Overview;
    app
}

fn draw(app: &mut App, w: u16, h: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

/// The issue number in the title of the panel drawn with the active
/// (focus) border; `None` unless exactly one panel has it.
fn highlighted_issue(app: &App, buf: &Buffer) -> Option<u64> {
    let active = app.active_theme().border_active;
    let area = buf.area;
    let corners: Vec<(u16, u16)> = (area.top()..area.bottom())
        .flat_map(|y| (area.left()..area.right()).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let c = &buf[(x, y)];
            c.symbol() == "╔" && c.fg == active
        })
        .collect();
    if corners.len() != 1 {
        return None;
    }
    let (x0, y) = corners[0];
    let title: String = (x0..area.right())
        .map(|x| buf[(x, y)].symbol().to_string())
        .take_while(|s| s != "╗")
        .collect();
    let digits: String = title
        .split('#')
        .nth(1)
        .unwrap_or_default()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn selected_issue(app: &App) -> Option<u64> {
    app.pool
        .session_at_index(app.panel_view.selected_index())
        .and_then(|s| s.issue_number)
}

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

#[tokio::test]
async fn overview_arrow_keeps_highlight_and_selection_in_sync() {
    // Prove the case: at 80x24 the body fits fewer rows than the terminal.
    let mut probe = overview_app(10);
    draw(&mut probe, 80, 24);
    let body = probe
        .panel_view
        .drawn_area
        .expect("Overview records its area");
    assert_ne!(
        GridLayout::calculate(10, 80, 24),
        GridLayout::calculate(10, body.width, body.height),
        "full-terminal and body layouts must differ for this test"
    );

    let keys = [
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Down,
        KeyCode::Char(']'),
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Char(']'),
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Up,
        KeyCode::Char('['),
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Char('['),
    ];
    for (w, h) in [(80, 24), (80, 27)] {
        for n in 1..=12 {
            let mut app = overview_app(n);
            let buf = draw(&mut app, w, h);
            assert_eq!(
                highlighted_issue(&app, &buf),
                Some(FIRST_ISSUE),
                "{n} at {w}x{h}"
            );
            for (step, code) in keys.iter().enumerate() {
                press(&mut app, *code).await;
                assert_eq!(app.tui_mode, TuiMode::Overview);
                let buf = draw(&mut app, w, h);
                let selected = selected_issue(&app);
                assert!(selected.is_some(), "{n} at {w}x{h}, step {step}");
                assert_eq!(
                    highlighted_issue(&app, &buf),
                    selected,
                    "{n} sessions at {w}x{h}, step {step} ({code:?})"
                );
            }
        }
    }
}

fn task(issue: u64) -> Session {
    Session::new(
        format!("task {issue}"),
        "claude-opus-4-5".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    )
}

/// The session `k` / Enter / `f` / `l` / `c` act on is the highlighted one.
fn assert_target_is_highlighted(app: &mut App, w: u16, h: u16, what: &str) {
    let buf = draw(app, w, h);
    let selected = selected_issue(app);
    assert!(selected.is_some(), "{what}: no session selected");
    assert_eq!(highlighted_issue(app, &buf), selected, "{what}");
}

#[tokio::test]
async fn overview_session_added_after_selecting_keeps_highlight_on_target() {
    for (w, h) in [(80, 24), (80, 27)] {
        let mut app = overview_app(4);
        draw(&mut app, w, h);
        press(&mut app, KeyCode::Down).await;
        press(&mut app, KeyCode::Right).await;
        assert_target_is_highlighted(&mut app, w, h, "4 sessions");
        app.pool.enqueue(task(FIRST_ISSUE + 4));
        assert_target_is_highlighted(&mut app, w, h, &format!("5th added at {w}x{h}"));
    }
}

#[tokio::test]
async fn overview_resize_keeps_highlight_on_target() {
    let mut app = overview_app(5);
    draw(&mut app, 80, 27);
    press(&mut app, KeyCode::Down).await;
    press(&mut app, KeyCode::Right).await;
    assert_target_is_highlighted(&mut app, 80, 27, "80x27");
    assert_target_is_highlighted(&mut app, 120, 27, "resized to 120x27");
    assert_target_is_highlighted(&mut app, 80, 24, "resized to 80x24");
}

#[tokio::test]
async fn overview_dismiss_on_a_later_page_does_not_panic_the_draw() {
    for (w, h) in [(80, 24), (80, 27)] {
        let mut app = overview_app(0);
        // 8 finished sessions, then 4 queued ones that `D` keeps.
        for i in 0..8 {
            app.pool.enqueue(task(FIRST_ISSUE + i));
            for id in app.pool.try_promote() {
                app.pool.get_session_mut(id).unwrap().status =
                    crate::session::types::SessionStatus::Completed;
                app.pool.finalize(id);
            }
        }
        for i in 8..12 {
            app.pool.enqueue(task(FIRST_ISSUE + i));
        }
        assert_eq!(app.pool.total_count(), 12);
        draw(&mut app, w, h);
        press(&mut app, KeyCode::Char(']')).await;
        press(&mut app, KeyCode::Char(']')).await;
        assert!(app.panel_view.grid_state.current_page > 0, "{w}x{h}");
        press(&mut app, KeyCode::Char('D')).await;
        assert_eq!(app.pool.total_count(), 4, "D dismissed the finished ones");
        assert_target_is_highlighted(&mut app, w, h, &format!("after D at {w}x{h}"));
    }
}
