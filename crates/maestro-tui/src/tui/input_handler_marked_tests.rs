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
/// shows, #1288) still kills the marked #2 after #1, above it, finishes.
#[tokio::test]
async fn detail_x_targets_marked_id_after_session_finishes() {
    let (mut app, ids) = running_app(3);
    draw(&mut app);
    press(&mut app, KeyCode::Char('j')).await;
    draw(&mut app);
    assert_eq!(
        app.screen_state.dashboard.list.selected_session(),
        Some(ids[1])
    );
    press(&mut app, KeyCode::F(6)).await;
    assert_eq!(app.tui_mode, TuiMode::DependencyGraph);
    draw(&mut app);

    finish(&mut app, ids[0]);
    press(&mut app, KeyCode::Char('X')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmKill(ids[1]), "not #3");
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
