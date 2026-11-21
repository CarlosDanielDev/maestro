//! #1261: kill from the Dashboard asks for the session's issue number.
//! #1280: the kill key is `X` (`k` is "up" on the Dashboard list).
//! `⏎` is inert until the typed text matches; `y` and `n` are just text.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::{handle_key, is_text_input_mode};
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::{App, TuiMode};

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

async fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        press(app, KeyCode::Char(c)).await;
    }
}

/// Overview with one live session for issue #407 selected.
fn app_with_407() -> App {
    let mut app = crate::tui::make_test_app("kill-typed-confirm");
    app.tui_mode = TuiMode::Overview;
    let mut s = Session::new(
        "auth refactor".into(),
        "claude-opus-4-5".into(),
        "orchestrator".into(),
        Some(407),
        None,
    );
    s.status = SessionStatus::Running;
    app.pool.enqueue(s);
    app
}

fn kill_attempted(app: &App) -> bool {
    app.activity_log
        .entries()
        .iter()
        .any(|e| e.session_label == "#407" && e.message.contains("not found or already finished"))
}

fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

#[tokio::test]
async fn kill_requires_issue_number() {
    let mut app = app_with_407();
    press(&mut app, KeyCode::Char('X')).await;
    assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    assert!(is_text_input_mode(&app), "typing must not fire global keys");
    let out = draw(&mut app);
    assert!(out.contains("Type 407 to confirm"), "{out}");

    // The old confirm keys and a reflex Enter do nothing.
    type_str(&mut app, "y").await;
    press(&mut app, KeyCode::Enter).await;
    assert!(matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    assert!(!kill_attempted(&app));

    press(&mut app, KeyCode::Backspace).await;
    type_str(&mut app, "40").await;
    press(&mut app, KeyCode::Enter).await;
    assert!(!kill_attempted(&app));
    let out = draw(&mut app);
    assert!(out.contains("2 of 3") && out.contains("locked"), "{out}");

    type_str(&mut app, "7").await;
    let out = draw(&mut app);
    assert!(!out.contains("locked"), "{out}");
    press(&mut app, KeyCode::Enter).await;
    assert!(kill_attempted(&app));
    assert!(!matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
}

#[tokio::test]
async fn kill_esc_cancels_and_next_open_starts_empty() {
    let mut app = app_with_407();
    press(&mut app, KeyCode::Char('X')).await;
    type_str(&mut app, "40").await;
    press(&mut app, KeyCode::Esc).await;
    assert!(!matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
    assert!(!kill_attempted(&app));

    press(&mut app, KeyCode::Char('X')).await;
    let out = draw(&mut app);
    assert!(out.contains("0 of 3"), "{out}");
}
