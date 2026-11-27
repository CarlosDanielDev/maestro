//! #1486: the session keys that still run off the Dashboard (`X`, `f`,
//! `⏎`, `c` on the Dependency graph) act on the session the Dashboard
//! marked, by id. A flat list index moved when a session above the mark
//! finished, or a queued one was promoted, before the Dashboard drew again.
//! Drawn at 120x36 (above the 80x24 SizeGuard) through the real key path.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::handle_key;
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::{App, SessionView, TuiMode};

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

fn draw(app: &mut App) {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
}

fn live(issue: u64) -> Session {
    let mut s = Session::new(
        format!("task {issue}"),
        "claude-opus-4-5".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    );
    s.status = SessionStatus::Running;
    s
}

/// Dashboard with live sessions #1..=#n promoted into the active slots.
fn running_app(n: u64) -> (App, Vec<uuid::Uuid>) {
    let mut app = crate::tui::make_test_app("marked-session-1486");
    let mut ids = Vec::new();
    for i in 1..=n {
        let s = live(i);
        ids.push(s.id);
        app.pool.enqueue(s);
    }
    let _ = app.pool.try_promote();
    // Promotion may move the status on; pin them live.
    for id in &ids {
        app.pool.get_session_mut(*id).unwrap().status = SessionStatus::Running;
    }
    app.tui_mode = TuiMode::Dashboard;
    (app, ids)
}

/// The session leaves the active slots for the finished list.
fn finish(app: &mut App, id: uuid::Uuid) {
    app.pool.get_session_mut(id).unwrap().status = SessionStatus::Completed;
    assert!(app.pool.finalize(id));
}

/// `X` off the Dashboard (Detail's own `X` already kills the session it
/// shows, #1288) still kills the marked session after #1, above it,
/// finishes: #2 (a flat index 1 lands on #3) and #3 (a flat index 2 lands
/// on #1, which has finished, so `X` opens no kill; this case also fails
/// a stale index 0, which lands on #2).
#[tokio::test]
async fn detail_x_targets_marked_id_after_session_finishes() {
    for marked in [1, 2] {
        let (mut app, ids) = running_app(3);
        draw(&mut app);
        for _ in 0..marked {
            press(&mut app, KeyCode::Char('j')).await;
        }
        draw(&mut app);
        assert_eq!(
            app.screen_state.dashboard.list.selected_session(),
            Some(ids[marked])
        );
        press(&mut app, KeyCode::F(6)).await;
        assert_eq!(app.tui_mode, TuiMode::DependencyGraph);
        draw(&mut app);

        finish(&mut app, ids[0]);
        press(&mut app, KeyCode::Char('X')).await;
        assert_eq!(
            app.tui_mode,
            TuiMode::ConfirmKill(ids[marked]),
            "#{}",
            marked + 1
        );
    }
}

/// `⏎` off the Dashboard opens the marked queued #4 after it is promoted
/// into the slot #1 freed (the promotion moves it in the list).
#[tokio::test]
async fn enter_after_promotion_opens_same_session() {
    let (mut app, ids) = running_app(3);
    let queued = live(4);
    let queued_id = queued.id;
    app.pool.enqueue(queued);
    finish(&mut app, ids[0]);
    draw(&mut app);
    // The Sessions rows are #2, #3, #4.
    press(&mut app, KeyCode::Char('j')).await;
    press(&mut app, KeyCode::Char('j')).await;
    draw(&mut app);
    assert_eq!(
        app.screen_state.dashboard.list.selected_session(),
        Some(queued_id)
    );
    press(&mut app, KeyCode::F(6)).await;
    assert_eq!(app.tui_mode, TuiMode::DependencyGraph);
    draw(&mut app);

    assert_eq!(app.pool.try_promote(), vec![queued_id]);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(
        app.tui_mode,
        TuiMode::Session {
            id: queued_id,
            view: SessionView::Detail
        },
        "not #1"
    );
}

/// #1509: on the Dependency graph a digit opens the Nth session row the
/// Dashboard shows, not the Nth session in pool order. Finished #1 sits
/// between the live sessions and queued #4 in the pool, so pool index 2 is
/// #1 while the third Sessions row is #4.
#[tokio::test]
async fn digit_key_selects_shown_row_by_id() {
    let (mut app, ids) = running_app(3);
    let queued = live(4);
    let queued_id = queued.id;
    app.pool.enqueue(queued);
    finish(&mut app, ids[0]);
    draw(&mut app);
    assert_eq!(app.pool.session_id_at_index(2), Some(ids[0]));
    press(&mut app, KeyCode::F(6)).await;
    assert_eq!(app.tui_mode, TuiMode::DependencyGraph);
    draw(&mut app);

    press(&mut app, KeyCode::Char('3')).await;
    assert_eq!(
        app.tui_mode,
        TuiMode::Session {
            id: queued_id,
            view: SessionView::Detail
        },
        "the third row shown is #4, not pool index 2 (#1)"
    );
}

/// #1509: `⏎` on a Home Needs-you session marks it on the Dashboard by id.
/// A pool index taken at the key moved when #1, above it, finished before
/// the Dashboard drew: index 2 then named #1.
#[tokio::test]
async fn home_needs_you_jump_marks_by_id() {
    let (mut app, ids) = running_app(3);
    app.pool.get_session_mut(ids[2]).unwrap().status = SessionStatus::FailedGates;
    press(&mut app, KeyCode::Char('1')).await;
    assert_eq!(app.tui_mode, TuiMode::Landing);
    draw(&mut app);

    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, TuiMode::Dashboard);
    finish(&mut app, ids[0]);
    draw(&mut app);
    assert_eq!(
        app.screen_state.dashboard.list.selected_session(),
        Some(ids[2]),
        "#3, not #1"
    );
}

/// #1509: a digit on a row `D` dismissed after the Dashboard drew opens
/// nothing (the fourth row shown, finished #1, is gone from the pool), and
/// a toast says why: a shown key never does nothing silently.
#[tokio::test]
async fn digit_key_on_a_dismissed_row_opens_nothing() {
    let (mut app, ids) = running_app(3);
    app.pool.enqueue(live(4));
    finish(&mut app, ids[0]);
    draw(&mut app);
    press(&mut app, KeyCode::F(6)).await;
    draw(&mut app);
    press(&mut app, KeyCode::Char('D')).await;
    assert!(app.pool.get_session(ids[0]).is_none(), "#1 was dismissed");

    press(&mut app, KeyCode::Char('4')).await;
    assert_eq!(app.tui_mode, TuiMode::DependencyGraph);
    let toast = app.toasts.visible(std::time::Instant::now());
    assert!(
        toast.is_some_and(|t| t.message == "Row 4 was dismissed: nothing to open"),
        "the dismissed row must say why"
    );
}
