use super::*;
use crate::session::types::SessionStatus;
use crate::tui::input_handler::handle_key;
use crate::tui::screens::test_helpers::key_event;
use crossterm::event::{KeyCode, KeyModifiers};

fn evidence() -> HollowEvidence {
    HollowEvidence {
        cost_usd: 0.0,
        files_touched: 0,
        tool_calls: false,
        elapsed_secs: Some(9),
    }
}

fn screen(id: Uuid) -> HollowRetryScreen {
    let cost = Cost::Paid { est: 0.5, cap: 5.0 };
    HollowRetryScreen::new(id, "S-test".into(), (1, 1), evidence(), cost)
}

// The card replaces the old single keys: `r` / `s` choose nothing.
#[test]
fn single_keys_choose_nothing() {
    let id = Uuid::new_v4();
    let mut s = screen(id);
    for c in ['r', 's', 'a'] {
        let action = s.handle_input(&key_event(KeyCode::Char(c)), InputMode::Normal);
        assert_eq!(action, ScreenAction::None, "`{c}`");
    }
}

#[test]
fn enter_on_default_skips() {
    let id = Uuid::new_v4();
    let mut s = screen(id);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::SkipHollow(id));
}

#[test]
fn down_then_enter_retries() {
    let id = Uuid::new_v4();
    let mut s = screen(id);
    s.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::RetryHollow(id));
}

#[test]
fn esc_decides_later() {
    let id = Uuid::new_v4();
    let action = screen(id).handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
}

#[test]
fn v_key_returns_push_detail() {
    let id = Uuid::new_v4();
    let action = screen(id).handle_input(&key_event(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(action, ScreenAction::Push(TuiMode::Detail(id)));
}

#[test]
fn card_lists_the_four_checks() {
    let mut s = screen(Uuid::new_v4());
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    t.draw(|f| s.draw(f, f.area(), &Theme::dark())).unwrap();
    let out = format!("{}", t.backend());
    for text in [
        "$0.00",
        "none touched",
        "tool calls",
        "9 s (under 30 s)",
        "1 of 1 auto-retries used",
        "est ≥ $0.50 · stops at $5.00",
        "decide later",
    ] {
        assert!(out.contains(text), "missing {text:?}:\n{out}");
    }
}

fn dismissed(app: &crate::tui::app::App, id: Uuid) -> bool {
    app.pool.get_session(id).unwrap().hollow_dismissed
}

/// #1287: the non-deciding keys (↑↓ pick, `v` evidence, `Esc` later) never
/// dismiss the hollow dialog for good; Skip does.
#[tokio::test]
async fn hollow_decision_card_skip_only() {
    let mut app = crate::tui::make_test_app("hollow-1287");
    app.no_splash = true;
    let mut s = Session::new(
        "work".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(965),
        None,
    );
    s.status = SessionStatus::Completed;
    s.is_hollow_completion = true;
    let id = s.id;
    app.pool.enqueue(s);
    app.pool.try_promote();
    app.pool.finalize_and_teardown(id);
    let open = |app: &mut crate::tui::app::App| {
        app.screen_state.hollow_retry_screen = Some(HollowRetryScreen::new(
            id,
            "#965".into(),
            (2, 2),
            evidence(),
            Cost::Free,
        ));
        app.navigate_to(TuiMode::HollowRetry);
    };
    let press = |code| KeyEvent::new(code, KeyModifiers::NONE);

    open(&mut app);
    handle_key(&mut app, press(KeyCode::Down)).await;
    handle_key(&mut app, press(KeyCode::Up)).await;
    assert!(!dismissed(&app, id), "picking does not dismiss");
    handle_key(&mut app, press(KeyCode::Char('v'))).await;
    assert!(!dismissed(&app, id), "evidence does not dismiss");
    app.navigate_to(TuiMode::HollowRetry);
    handle_key(&mut app, press(KeyCode::Esc)).await;
    assert_ne!(app.tui_mode, TuiMode::HollowRetry, "Esc leaves");
    assert!(!dismissed(&app, id), "Esc decides later, never for good");
    assert!(app.screen_state.hollow_later.contains(&id));
    app.check_completions().await.expect("check_completions");
    assert!(
        app.screen_state.hollow_retry_screen.is_none(),
        "later: the dialog does not reopen on its own"
    );

    open(&mut app);
    handle_key(&mut app, press(KeyCode::Enter)).await;
    assert!(dismissed(&app, id), "Skip dismisses");
    assert!(app.screen_state.hollow_retry_screen.is_none());
    assert_ne!(app.tui_mode, TuiMode::HollowRetry);
}
