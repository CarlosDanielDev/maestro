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

async fn ctrl_k_to(app: &mut App, query: &str) {
    handle_key(
        app,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
    )
    .await;
    for c in query.chars() {
        handle_key(app, key(c)).await;
    }
    handle_key(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
}

/// `b` must not strand the parked session. ⌃K cannot leave Settings (it
/// is a text-input mode, so the palette stays shut and the keys stay in
/// Settings); any other way off Settings (here the mode is switched to
/// Issues, as a route the key path does not offer would) opens the gate
/// again on the next key. Nothing spawns either way.
#[tokio::test]
async fn budget_gate_b_never_strands_the_parked_session() {
    let (mut app, _dir, id) = blocked_app().await;
    assert_eq!(
        app.pool.get_session(id).map(|s| s.status),
        Some(SessionStatus::Queued),
        "setup: the parked session is Queued"
    );
    handle_key(&mut app, key('b')).await;
    assert_eq!(app.tui_mode, TuiMode::Settings);
    ctrl_k_to(&mut app, "issues").await;
    assert!(app.palette.is_none(), "no palette over Settings");
    assert_eq!(
        app.tui_mode,
        TuiMode::Settings,
        "⌃K does not leave Settings"
    );

    app.tui_mode = TuiMode::IssueBrowser;
    handle_key(&mut app, key('x')).await;
    assert!(
        matches!(app.tui_mode, TuiMode::BudgetPreSpawn { session_id } if session_id == id),
        "the gate is back after leaving Settings; got {:?}",
        app.tui_mode
    );
    assert!(draw(&mut app).contains("would pass the budget cap"));
    assert!(!app.budget_skip_once.contains(&id), "nothing armed a spawn");
    assert_eq!(
        app.pool.get_session(id).map(|s| s.status),
        Some(SessionStatus::Queued),
        "nothing spawned"
    );
}

/// `n` drops exactly the session the gate shows; the other session and
/// the spend are left alone, and nothing spawns.
#[tokio::test]
async fn budget_gate_n_drops_only_the_shown_session() {
    let (mut app, _dir, id) = blocked_app().await;
    let others: Vec<_> = (app.pool.all_sessions().into_iter())
        .filter(|s| s.id != id)
        .map(|s| (s.id, s.status))
        .collect();
    assert_eq!(others.len(), 1, "setup: one other session");
    handle_key(&mut app, key('n')).await;
    assert!(
        !matches!(app.tui_mode, TuiMode::BudgetPreSpawn { .. }),
        "n answers the gate"
    );
    assert_eq!(
        app.pool.get_session(id).map(|s| s.status),
        Some(SessionStatus::Killed),
        "the shown session is dropped"
    );
    for (other, status) in others {
        assert_eq!(
            app.pool.get_session(other).map(|s| s.status),
            Some(status),
            "the other session is untouched"
        );
    }
    assert!(!app.budget_skip_once.contains(&id));
    assert!(
        !app.activity_log
            .entries()
            .iter()
            .any(|e| e.message.contains("post budget gate")),
        "nothing spawned"
    );
}

/// The gate never comes back over a text field mid-typing or over Confirm
/// Exit (and its ⌃Q hold); it waits until they are left.
#[tokio::test]
async fn budget_gate_return_waits_for_text_fields_and_confirm_exit() {
    let (mut app, _dir, id) = blocked_app().await;
    handle_key(&mut app, key('b')).await;
    for mode in [TuiMode::PromptInput, TuiMode::ConfirmExit] {
        app.tui_mode = mode;
        handle_key(&mut app, key('x')).await;
        assert_eq!(app.tui_mode, mode, "the gate waits over {mode:?}");
        assert_eq!(app.budget_gate_return, Some(id), "still remembered");
    }
    app.tui_mode = TuiMode::IssueBrowser;
    handle_key(&mut app, key('x')).await;
    assert!(
        matches!(app.tui_mode, TuiMode::BudgetPreSpawn { session_id } if session_id == id),
        "back once they are left; got {:?}",
        app.tui_mode
    );
}
