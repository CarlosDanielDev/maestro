//! #1276 (§02.24): the budget gate's keys on Warn and its `b`, on the same
//! production-built App as `budget_tests.rs`.

use super::*;

/// A Warn gate: $0.35 of a $1 cap spent, so the next turn ($0.50, the
/// 10% floor of the $5 per-session cap) passes the 80% alert, not the cap.
async fn warn_app() -> (App, TempDir, uuid::Uuid) {
    let (mut app, dir) = production_app(1.0);
    let spender = fresh_session();
    let spender_id = spender.id;
    app.pool.enqueue(spender);
    let _ = app.pool.try_promote();
    app.pool
        .get_active_mut(spender_id)
        .unwrap()
        .session
        .cost_usd = 0.35;
    app.sync_state();
    let session = fresh_session();
    let id = session.id;
    app.add_session(session).await.unwrap();
    assert!(
        matches!(app.tui_mode, TuiMode::BudgetPreSpawn { session_id } if session_id == id),
        "setup: Warn opens the gate; got {:?}",
        app.tui_mode
    );
    (app, dir, id)
}

/// On Warn `S` spawns; lowercase `y` / `s` (the old twins) do not.
#[tokio::test]
async fn budget_gate_spawns_on_warn_with_uppercase_s_only() {
    let (mut app, _dir, id) = warn_app().await;
    let out = draw(&mut app);
    assert!(out.contains("WARN") && out.contains("S spawn now"), "{out}");
    for c in ['y', 's'] {
        handle_key(&mut app, key(c)).await;
        assert!(
            matches!(app.tui_mode, TuiMode::BudgetPreSpawn { .. }),
            "`{c}` no longer spawns"
        );
    }
    handle_key(&mut app, key('S')).await;
    assert!(app.budget_skip_once.contains(&id), "`S` spawns on Warn");
    assert!(!matches!(app.tui_mode, TuiMode::BudgetPreSpawn { .. }));
}

/// `b` opens Settings › Budget over the gate; `Esc` comes back to it.
#[tokio::test]
async fn budget_gate_b_opens_settings_budget_and_esc_returns() {
    let (mut app, _dir, id) = blocked_app().await;
    handle_key(&mut app, key('b')).await;
    assert_eq!(app.tui_mode, TuiMode::Settings);
    let tab = app
        .screen_state
        .settings_screen
        .as_ref()
        .map(|s| s.active_tab());
    assert_eq!(
        tab,
        Some(crate::tui::screens::settings::SettingsTab::Budget)
    );
    handle_key(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).await;
    assert!(
        matches!(app.tui_mode, TuiMode::BudgetPreSpawn { session_id } if session_id == id),
        "Esc returns to the gate; got {:?}",
        app.tui_mode
    );
}
