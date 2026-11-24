//! #1280 judge round: every key the Dashboard took over from Overview acts
//! on the marked row and stays reachable. Drawn at 120x36 (above the 80x24
//! SizeGuard) through the real key path.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::rows::WorkKey;
use crate::provider::github::ci::PendingPrCheck;
use crate::provider::github::types::{PendingPrStatus, awaiting_pending_pr};
use crate::provider::types::{CheckConclusion, CheckRun, CheckStatus};
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

/// Dashboard with live sessions #1..=#n, drawn once.
fn sessions_app(n: u64) -> (App, Vec<uuid::Uuid>) {
    let mut app = crate::tui::make_test_app("dashboard-1280-reach");
    // As in the app: the launcher answers the keys the Dashboard leaves, so
    // nothing falls through to the Overview handlers.
    app.screen_state.home_screen = Some(crate::tui::screens::HomeScreen::new(
        crate::tui::screens::home::ProjectInfo {
            repo: "owner/repo".into(),
            branch: "main".into(),
            username: None,
        },
        vec![],
        vec![],
    ));
    let mut ids = Vec::new();
    for i in 1..=n {
        let s = live(i);
        ids.push(s.id);
        app.pool.enqueue(s);
    }
    app.tui_mode = TuiMode::Dashboard;
    draw(&mut app);
    (app, ids)
}

/// B1: Detail opened from the Work list kills, and opens Fullscreen for,
/// the session it shows, not the old grid selection.
#[tokio::test]
async fn detail_from_dashboard_acts_on_the_session_it_shows() {
    for code in [KeyCode::Char('X'), KeyCode::F(10)] {
        let (mut app, ids) = sessions_app(2);
        press(&mut app, KeyCode::Char('j')).await;
        press(&mut app, KeyCode::Enter).await;
        assert_eq!(app.tui_mode, TuiMode::Detail(ids[1]));
        draw(&mut app);
        press(&mut app, code).await;
        assert_eq!(app.tui_mode, TuiMode::ConfirmKill(ids[1]), "{code:?}");
        let out = draw(&mut app);
        assert!(out.contains("Type 2 to confirm"), "{code:?}\n{out}");
    }
    let (mut app, ids) = sessions_app(2);
    press(&mut app, KeyCode::Char('j')).await;
    press(&mut app, KeyCode::Enter).await;
    press(&mut app, KeyCode::Char('f')).await;
    assert_eq!(app.tui_mode, TuiMode::Fullscreen(ids[1]));
}

/// `k` in Detail no longer kills: kill is `X` there too, as on the list.
#[tokio::test]
async fn detail_k_does_not_kill() {
    let (mut app, ids) = sessions_app(2);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, TuiMode::Detail(ids[0]));
    press(&mut app, KeyCode::Char('k')).await;
    assert!(!matches!(app.tui_mode, TuiMode::ConfirmKill(_)));
}

/// X and ⏎ after arrow moves act on the marked row.
#[tokio::test]
async fn x_and_enter_after_moves_act_on_the_marked_row() {
    let (mut app, ids) = sessions_app(3);
    press(&mut app, KeyCode::Down).await;
    press(&mut app, KeyCode::Char('j')).await;
    press(&mut app, KeyCode::Up).await;
    let out = draw(&mut app);
    let marked: Vec<&str> = out.lines().filter(|l| l.contains('▌')).collect();
    assert_eq!(marked.len(), 1, "{out}");
    assert!(marked[0].contains("#2"), "{out}");
    press(&mut app, KeyCode::Char('X')).await;
    assert_eq!(app.tui_mode, TuiMode::ConfirmKill(ids[1]));
    press(&mut app, KeyCode::Esc).await;
    draw(&mut app);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, TuiMode::Detail(ids[1]));
}

/// D dismisses the finished sessions from the Dashboard.
#[tokio::test]
async fn d_dismisses_finished_sessions() {
    let (mut app, _) = sessions_app(1);
    let mut done = live(9);
    done.status = SessionStatus::Completed;
    app.pool.enqueue(done);
    for id in app.pool.try_promote() {
        app.pool.finalize(id);
    }
    let before = app.pool.total_count();
    draw(&mut app);
    press(&mut app, KeyCode::Char('D')).await;
    assert!(app.pool.total_count() < before, "D dismissed nothing");
    assert_eq!(app.tui_mode, TuiMode::Dashboard);
}

/// B2: `e` opens the CI Error Review from the Dashboard when a check fails.
#[tokio::test]
async fn e_opens_ci_error_review_on_the_dashboard() {
    let (mut app, _) = sessions_app(0);
    app.ci_poller.add_check(PendingPrCheck {
        pr_number: 42,
        issue_number: 10,
        branch: "maestro/issue-10".into(),
        created_at: Instant::now(),
        check_count: 0,
        fix_attempt: 0,
        awaiting_fix_ci: false,
    });
    app.ci_poller.ci_check_details.insert(
        42,
        vec![CheckRun {
            name: "clippy".into(),
            status: CheckStatus::Completed,
            conclusion: CheckConclusion::Failure,
            started_at: None,
            elapsed_secs: None,
        }],
    );
    draw(&mut app);
    press(&mut app, KeyCode::Char('e')).await;
    assert_eq!(app.tui_mode, TuiMode::CiErrorReview);
}

/// Nit: a row whose create is running or queued never arms (no second
/// `gh pr create`).
#[tokio::test]
async fn retrying_and_scheduled_rows_never_arm() {
    for status in [PendingPrStatus::Retrying, PendingPrStatus::RetryScheduled] {
        let (mut app, _) = sessions_app(0);
        let mut p = awaiting_pending_pr(545);
        p.status = status;
        app.pending_prs.push(p);
        draw(&mut app);
        app.screen_state.dashboard.clock = Some(Instant::now());
        press(&mut app, KeyCode::Enter).await;
        assert_eq!(app.screen_state.dashboard.armed_for, None, "{status:?}");
        press(&mut app, KeyCode::Enter).await;
        assert_eq!(app.pending_prs[0].manual_retry_count, 0, "{status:?}");
        assert_eq!(app.pending_prs[0].status, status);
    }
}

/// Nit: a key that never reaches the Dashboard (a tab switch) disarms too.
#[tokio::test]
async fn leaving_the_dashboard_disarms() {
    let (mut app, _) = sessions_app(0);
    app.pending_prs.push(awaiting_pending_pr(545));
    draw(&mut app);
    app.screen_state.dashboard.clock = Some(Instant::now());
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.screen_state.dashboard.armed_for, Some(545));
    press(&mut app, KeyCode::Char('1')).await;
    press(&mut app, KeyCode::Char('2')).await;
    draw(&mut app);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(
        app.pending_prs[0].manual_retry_count, 0,
        "the tab switch disarmed, so this ⏎ only arms"
    );
    assert_eq!(
        app.screen_state.dashboard.list.selected,
        Some(WorkKey::PendingPr(545))
    );
}

/// B2, #1292: the Agent graph is the Files lens of Deps. The ⌃K entry
/// opens it; so does `g g` from the Dashboard. With
/// `views.agent_graph_enabled = false` the entry says why and the second
/// `g` stays on issues.
#[tokio::test]
async fn palette_opens_the_agent_graph() {
    use super::deps::{Lens, lens};
    use super::usage::View;
    for enabled in [true, false] {
        let (mut app, _) = sessions_app(1);
        let toml = format!(
            "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\n\
             total_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n\
             [views]\nagent_graph_enabled = {enabled}\n"
        );
        app.config = Some(toml::from_str(&toml).expect("config parses"));
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        )
        .await;
        for c in "agent graph".chars() {
            press(&mut app, KeyCode::Char(c)).await;
        }
        press(&mut app, KeyCode::Enter).await;
        app.palette = None;
        let dash = &app.screen_state.dashboard;
        assert_eq!(app.tui_mode, TuiMode::Dashboard, "enabled={enabled}");
        if enabled {
            assert_eq!((dash.view, lens(dash, true)), (View::Deps, Lens::Files));
        } else {
            assert_ne!(dash.view, View::Deps, "a disabled entry runs nothing");
        }
        // From Ops, `g` reopens Deps on its last lens (files after the
        // palette; issues when the flag is off).
        press(&mut app, KeyCode::Char('o')).await;
        press(&mut app, KeyCode::Char('g')).await;
        if !enabled {
            press(&mut app, KeyCode::Char('g')).await;
        }
        let want = if enabled { Lens::Files } else { Lens::Issues };
        assert_eq!(app.tui_mode, TuiMode::Dashboard);
        assert_eq!(app.screen_state.dashboard.view, View::Deps);
        assert_eq!(
            lens(&app.screen_state.dashboard, enabled),
            want,
            "enabled={enabled}"
        );
    }
}
