//! #1280: Dashboard keys through the real key path, drawn at 120x36
//! (below 80x24 the SizeGuard swallows keys).

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::rows::WorkKey;
use crate::provider::github::types::{PendingPrStatus, awaiting_pending_pr};
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

fn live_session(issue: u64) -> Session {
    let mut s = Session::new(
        "auth refactor".into(),
        "claude-opus-4-5".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    );
    s.status = SessionStatus::Running;
    s
}

fn dashboard_app() -> App {
    let mut app = crate::tui::make_test_app("dashboard-1280");
    app.tui_mode = TuiMode::Dashboard;
    app
}

fn set_clock(app: &mut App, at: Instant) {
    app.screen_state.dashboard.clock = Some(at);
}

#[tokio::test]
async fn dashboard_pending_pr_armed_create() {
    let mut app = dashboard_app();
    app.gh_auth_ok = true;
    app.pending_prs.push(awaiting_pending_pr(545));
    draw(&mut app);
    assert_eq!(
        app.screen_state.dashboard.list.selected,
        Some(WorkKey::PendingPr(545))
    );
    let t0 = Instant::now();
    let retries = |app: &App| app.pending_prs[0].manual_retry_count;

    // First ⏎ arms and writes nothing; the bar shows the exact write.
    set_clock(&mut app, t0);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(retries(&app), 0, "one ⏎ must not create the PR");
    assert_eq!(
        app.pending_prs[0].status,
        PendingPrStatus::AwaitingManualRetry
    );
    let out = draw(&mut app);
    assert!(out.contains("ARMED"), "{out}");
    assert!(
        out.contains("gh pr create --head maestro/issue-545"),
        "{out}"
    );

    // Any other key disarms.
    press(&mut app, KeyCode::Char('j')).await;
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(retries(&app), 0, "j disarmed, so this ⏎ only arms again");

    // A second ⏎ later than 3s re-arms instead of writing.
    set_clock(&mut app, t0 + Duration::from_secs(4));
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(retries(&app), 0, "a late ⏎ must not write");

    // A second ⏎ within 3s queues the create.
    set_clock(&mut app, t0 + Duration::from_secs(6));
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(retries(&app), 1);
    assert_eq!(app.pending_prs[0].status, PendingPrStatus::RetryScheduled);
    assert_eq!(app.tui_mode, TuiMode::Dashboard);
}

#[tokio::test]
async fn dashboard_pending_pr_stopped_row_never_arms() {
    let mut app = dashboard_app();
    let mut p = awaiting_pending_pr(42);
    p.status = PendingPrStatus::PermanentlyFailed;
    app.pending_prs.push(p);
    draw(&mut app);
    set_clock(&mut app, Instant::now());
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(
        app.screen_state.dashboard.armed_for, None,
        "a stopped row never arms"
    );
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.pending_prs[0].manual_retry_count, 0);
    let out = draw(&mut app);
    assert!(
        out.contains("BLOCKED") && out.contains("retries stopped"),
        "{out}"
    );
}

/// gh signed out does not lock the row: the retry is the gh call that
/// turns `gh_auth_ok` true again.
#[tokio::test]
async fn dashboard_pending_pr_arms_while_gh_is_signed_out() {
    let mut app = dashboard_app();
    app.gh_auth_ok = false;
    app.pending_prs.push(awaiting_pending_pr(545));
    draw(&mut app);
    set_clock(&mut app, Instant::now());
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.pending_prs[0].manual_retry_count, 0);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.pending_prs[0].manual_retry_count, 1);
}

#[tokio::test]
async fn dashboard_kill_typed_confirm() {
    let mut app = dashboard_app();
    let s = live_session(407);
    let id = s.id;
    app.pool.enqueue(s);
    draw(&mut app);
    assert_eq!(app.screen_state.dashboard.list.selected_session(), Some(id));

    // `k` moves (it is "up" here), it never kills.
    press(&mut app, KeyCode::Char('k')).await;
    assert_eq!(app.tui_mode, TuiMode::Dashboard);

    press(&mut app, KeyCode::Char('X')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmKill(id));
    let out = draw(&mut app);
    assert!(out.contains("Type 407 to confirm"), "{out}");
    assert!(
        out.contains("Work (1)"),
        "the Dashboard stays under the dialog\n{out}"
    );

    // The old confirm keys and a reflex ⏎ do nothing.
    press(&mut app, KeyCode::Char('y')).await;
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmKill(id));
    press(&mut app, KeyCode::Esc).await;
    assert!(!matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
}

#[tokio::test]
async fn dashboard_kill_ignores_finished_sessions() {
    let mut app = dashboard_app();
    let mut s = live_session(396);
    s.status = SessionStatus::Killed;
    app.pool.enqueue(s);
    draw(&mut app);
    press(&mut app, KeyCode::Char('X')).await;
    assert_eq!(app.tui_mode, TuiMode::Dashboard);
}

#[tokio::test]
async fn overview_routes_to_dashboard() {
    let mut app = crate::tui::make_test_app("dashboard-1280-overview");
    app.pool.enqueue(live_session(412));
    app.navigate_to(TuiMode::Dashboard);
    let out = draw(&mut app);
    assert_eq!(app.tui_mode, TuiMode::Dashboard);
    assert!(out.contains("Work (1)"), "{out}");
    assert!(out.contains("SESSIONS"), "{out}");
    assert!(
        out.contains("PIPELINE"),
        "the session detail is beside the list\n{out}"
    );
}

#[tokio::test]
async fn esc_while_armed_only_disarms() {
    let mut app = dashboard_app();
    app.pending_prs.push(awaiting_pending_pr(545));
    app.nav_stack.push(TuiMode::Landing);
    draw(&mut app);
    set_clock(&mut app, Instant::now());
    press(&mut app, KeyCode::Enter).await;
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(
        app.tui_mode,
        TuiMode::Dashboard,
        "Esc disarms, it does not go back"
    );
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.pending_prs[0].manual_retry_count, 0, "Esc disarmed");
}
