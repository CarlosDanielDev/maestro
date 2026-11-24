//! #1289: the Usage views of the Dashboard (§02.14 of
//! `docs/design-system/index.html`): `c` Cost and `t` Tokens, through the
//! real key path and `ui::draw` (never below 80x24: the SizeGuard swallows
//! keys there).

use chrono::{Duration, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use super::{fixed_start, make_session};
use crate::config::BudgetConfig;
use crate::session::types::{Session, SessionStatus, TokenUsage};
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::screens::dashboard::rows::WorkKey;
use crate::tui::screens::dashboard::usage::{View, budget_thresholds};
use crate::tui::{make_test_app, ui};

const BUDGET_TOML: &str = "[project]\nrepo = \"owner/repo\"\n[sessions]\n\
    [budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
    [github]\n[notifications]\n";

pub(super) fn session(
    issue: u64,
    status: SessionStatus,
    model: &str,
    cost: f64,
    k: TokenUsage,
) -> Session {
    let mut s = make_session(status, Some(issue));
    s.id = Uuid::from_u128(u128::from(issue));
    s.model = model.into();
    s.cost_usd = cost;
    s.token_usage = k;
    s
}

pub(super) fn tokens(input: u64, output: u64, read: u64, write: u64) -> TokenUsage {
    TokenUsage {
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: read,
        cache_creation_tokens: write,
    }
}

/// No sessions, a `[budget]`, a pinned clock (the scope line and the rate).
pub(super) fn empty_app() -> App {
    let mut app = make_test_app("usage-1289");
    app.show_mascot = false;
    app.show_activity_log = false;
    app.gh_auth_ok = true;
    app.bypass_active = false;
    app.no_splash = true;
    app.config = Some(toml::from_str(BUDGET_TOML).expect("test config parse"));
    app.start_time = fixed_start();
    app.total_cost = 0.0;
    app.screen_state.dashboard.wall = Some(fixed_start() + Duration::minutes(90));
    app.tui_mode = TuiMode::Dashboard;
    app
}

/// Three live sessions (one on an unpriced model), one finished.
fn usage_app() -> App {
    let mut app = empty_app();
    let mut unpriced = session(
        414,
        SessionStatus::Running,
        "custom",
        0.0,
        tokens(22_000, 15_000, 310_000, 0),
    );
    unpriced.cost_unknown = true;
    unpriced.issue_title = Some("perf-bench".into());
    let mut failed = session(
        412,
        SessionStatus::FailedGates,
        "claude-opus-4-5",
        1.92,
        tokens(8_000, 9_000, 375_000, 30_000),
    );
    failed.issue_title = Some("auth-refactor".into());
    let mut running = session(
        409,
        SessionStatus::Running,
        "claude-sonnet-4-5",
        0.62,
        tokens(9_000, 12_000, 543_333, 40_000),
    );
    running.issue_title = Some("fleet-sort".into());
    running.tq_handoff_original_tokens = Some(45_000);
    running.tq_handoff_compressed_tokens = Some(12_000);
    let mut done = session(
        397,
        SessionStatus::Completed,
        "claude-sonnet-4-5",
        3.88,
        tokens(14_000, 22_000, 943_333, 60_000),
    );
    done.issue_title = Some("tab-bar".into());
    for s in [failed, running, unpriced, done] {
        app.pool.enqueue(s);
    }
    app.total_cost = 6.42;
    app
}

async fn press(app: &mut App, c: char) {
    handle_key(app, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)).await;
}

fn draw(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    t
}

fn text(t: &Terminal<TestBackend>) -> String {
    let buf = t.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn view(app: &App) -> View {
    app.screen_state.dashboard.view
}

#[tokio::test]
async fn usage_views_keep_selection() {
    let mut app = usage_app();
    draw(&mut app, 120, 36);
    press(&mut app, 'j').await;
    let picked = app.screen_state.dashboard.list.selected;
    assert_eq!(picked, Some(WorkKey::Session(Uuid::from_u128(409))));

    press(&mut app, 'c').await;
    assert_eq!((app.tui_mode, view(&app)), (TuiMode::Dashboard, View::Cost));
    let out = text(&draw(&mut app, 120, 36));
    assert_eq!(app.screen_state.dashboard.list.selected, picked);
    assert!(out.contains("Spend by session"), "{out}");
    let marked = out.lines().find(|l| l.contains('▌')).unwrap_or_default();
    assert!(
        marked.contains("#409"),
        "Cost marks the kept session: {out}"
    );

    press(&mut app, 't').await;
    assert_eq!(view(&app), View::Tokens);
    let out = text(&draw(&mut app, 120, 36));
    assert_eq!(app.screen_state.dashboard.list.selected, picked);
    assert!(out.contains("Tokens by session"), "{out}");

    // `j` moves the same selection in a Usage view.
    press(&mut app, 'j').await;
    let moved = app.screen_state.dashboard.list.selected;
    assert_ne!(moved, picked);

    press(&mut app, 'o').await;
    assert_eq!(view(&app), View::Ops);
    assert_eq!(app.screen_state.dashboard.list.selected, moved);

    // Esc in a Usage view goes back to Ops, not off the Dashboard.
    press(&mut app, 'c').await;
    handle_key(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).await;
    assert_eq!((app.tui_mode, view(&app)), (TuiMode::Dashboard, View::Ops));
    assert_eq!(app.screen_state.dashboard.list.selected, moved);
}

#[tokio::test]
async fn budget_thresholds_from_config() {
    let cfg = |total_usd, per_session_usd, alert_threshold_pct| BudgetConfig {
        per_session_usd,
        total_usd,
        alert_threshold_pct,
    };
    let at = |c: &BudgetConfig| budget_thresholds(c).map(|t| t.at_usd);
    assert_eq!(at(&cfg(50.0, 5.0, 80)), [40.0, 5.0, 50.0]);
    assert_eq!(at(&cfg(120.0, 7.5, 50)), [60.0, 7.5, 120.0]);
    // The words follow app/budget.rs: the cap pauses, it does not kill.
    let what = budget_thresholds(&cfg(50.0, 5.0, 80)).map(|t| t.what);
    assert!(what[1].contains("errored"), "{what:?}");
    assert!(
        what[2].contains("pause") && !what[2].contains("kill"),
        "{what:?}"
    );
    assert!(what[0].contains("banner"), "{what:?}");

    // The panel draws the configured amounts, not the defaults.
    let mut app = usage_app();
    let toml = BUDGET_TOML
        .replace("5.0", "7.5")
        .replace("50.0", "120.0")
        .replace("= 80", "= 50");
    app.config = Some(toml::from_str(&toml).expect("test config parse"));
    press(&mut app, 'c').await;
    let out = text(&draw(&mut app, 120, 36));
    for want in ["$60.00", "$7.50", "$120.00", "What happens"] {
        assert!(out.contains(want), "{want}: {out}");
    }
    assert!(!out.contains("$40.00"), "{out}");
}

/// The TurboQuant screen is gone: its ⌃K entry opens Dashboard › Tokens,
/// where TurboQuant is a tile and a panel. The old Cost and Tokens modes
/// (navigate_to, a direct assignment, F4 / F5) are aliases of the views.
#[tokio::test]
async fn turboquant_mode_redirects_to_tokens() {
    use crate::tui::navigation::registry::{PALETTE_COMMANDS, PaletteCommand};
    let (label, _, key_on) = PALETTE_COMMANDS
        .iter()
        .find(|(_, c, _)| *c == PaletteCommand::Turboquant)
        .expect("TurboQuant keeps a palette entry");
    assert_eq!(*key_on, Some(("t", TuiMode::Dashboard)));
    for from in [TuiMode::Landing, TuiMode::IssueBrowser] {
        let mut app = usage_app();
        app.tui_mode = from;
        draw(&mut app, 120, 36);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        )
        .await;
        for c in label.chars() {
            press(&mut app, c).await;
        }
        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
        assert_eq!(
            (app.tui_mode, view(&app)),
            (TuiMode::Dashboard, View::Tokens),
            "from {from:?}"
        );
        let out = text(&draw(&mut app, 120, 36));
        assert!(out.contains("TurboQuant · "), "the panel is drawn: {out}");
    }

    let mut app = usage_app();
    app.tui_mode = TuiMode::TokenDashboard;
    draw(&mut app, 120, 36);
    assert_eq!(
        (app.tui_mode, view(&app)),
        (TuiMode::Dashboard, View::Tokens)
    );

    let mut app = usage_app();
    app.navigate_to(TuiMode::CostDashboard);
    assert_eq!((app.tui_mode, view(&app)), (TuiMode::Dashboard, View::Cost));
    app.navigate_to(TuiMode::TokenDashboard);
    assert_eq!(
        (app.tui_mode, view(&app)),
        (TuiMode::Dashboard, View::Tokens)
    );

    let mut app = usage_app();
    draw(&mut app, 120, 36);
    handle_key(&mut app, KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE)).await;
    assert_eq!((app.tui_mode, view(&app)), (TuiMode::Dashboard, View::Cost));
    handle_key(&mut app, KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)).await;
    assert_eq!(
        (app.tui_mode, view(&app)),
        (TuiMode::Dashboard, View::Tokens)
    );
}

#[tokio::test]
async fn usage_cost_wide() {
    let mut app = usage_app();
    press(&mut app, 'c').await;
    assert_snapshot!(draw(&mut app, 160, 40).backend());
}

#[tokio::test]
async fn usage_cost_compact() {
    let mut app = usage_app();
    press(&mut app, 'c').await;
    assert_snapshot!(draw(&mut app, 90, 30).backend());
}

#[tokio::test]
async fn usage_tokens_wide() {
    let mut app = usage_app();
    press(&mut app, 't').await;
    assert_snapshot!(draw(&mut app, 160, 40).backend());
}

#[tokio::test]
async fn usage_tokens_compact() {
    let mut app = usage_app();
    press(&mut app, 't').await;
    assert_snapshot!(draw(&mut app, 90, 30).backend());
}

/// Before any run: what is already known (the cap rules) and what to do.
#[tokio::test]
async fn usage_empty() {
    let mut app = empty_app();
    press(&mut app, 'c').await;
    let t = draw(&mut app, 120, 30);
    let out = text(&t);
    assert!(out.contains("Nothing has run yet"), "{out}");
    assert!(
        out.contains("1 Home"),
        "names the key that starts work: {out}"
    );
    assert!(out.contains("What happens"), "{out}");
    assert_snapshot!(t.backend());
}

#[tokio::test]
async fn usage_scope_line_names_this_run() {
    let mut app = usage_app();
    press(&mut app, 'c').await;
    let out = text(&draw(&mut app, 120, 36));
    let start = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
    assert_eq!(app.start_time, start);
    assert!(out.contains("this run · since 10:00 UTC"), "{out}");
}

/// An old route pushed from another screen still comes back sanely: Esc
/// leaves the Usage view for Ops, then goes back to the screen it came from.
#[tokio::test]
async fn usage_alias_esc_goes_back_to_the_screen_it_came_from() {
    let esc = || KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let mut app = usage_app();
    app.tui_mode = TuiMode::IssueBrowser;
    draw(&mut app, 120, 36);
    app.navigate_to(TuiMode::CostDashboard);
    draw(&mut app, 120, 36);
    assert_eq!((app.tui_mode, view(&app)), (TuiMode::Dashboard, View::Cost));
    handle_key(&mut app, esc()).await;
    assert_eq!((app.tui_mode, view(&app)), (TuiMode::Dashboard, View::Ops));
    draw(&mut app, 120, 36);
    handle_key(&mut app, esc()).await;
    assert_eq!(app.tui_mode, TuiMode::IssueBrowser);
}
