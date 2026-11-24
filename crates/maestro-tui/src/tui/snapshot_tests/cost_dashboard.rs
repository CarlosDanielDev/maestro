//! #1289: Dashboard › Cost through `ui::draw` (it was the Cost screen).
//! The look is in the `usage_*` snapshots (`snapshot_tests/usage.rs`);
//! these pin what each old Cost case must still say.

use super::*;
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::TuiMode;
use crate::tui::{make_test_app, ui};
use ratatui::{Terminal, backend::TestBackend};

fn cost_view(sessions: Vec<Session>, total: f64, budget: Option<(f64, f64)>) -> String {
    let mut app = make_test_app("cost-view-1289");
    app.no_splash = true;
    app.start_time = fixed_start();
    app.screen_state.dashboard.wall = Some(fixed_end());
    if let Some((per, cap)) = budget {
        let toml = format!(
            "[project]\nrepo = \"o/r\"\n[sessions]\n[budget]\nper_session_usd = {per}\n\
             total_usd = {cap}\nalert_threshold_pct = 80\n[github]\n[notifications]\n"
        );
        app.config = Some(toml::from_str(&toml).expect("test config parse"));
    }
    for s in sessions {
        app.pool.enqueue(s);
    }
    app.total_cost = total;
    app.navigate_to(TuiMode::CostDashboard);
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    t.draw(|f| ui::draw(f, &mut app)).unwrap();
    format!("{}", t.backend())
}

fn titled(issue: u64, status: SessionStatus, title: &str, cost: f64) -> Session {
    let mut s = make_session(status, Some(issue));
    s.id = uuid::Uuid::from_u128(u128::from(issue));
    s.issue_title = Some(title.to_string());
    s.cost_usd = cost;
    s
}

#[test]
fn cost_dashboard_no_budget() {
    let out = cost_view(
        vec![titled(2, SessionStatus::Running, "Fix crash", 1.11)],
        1.23,
        None,
    );
    assert!(out.contains("No [budget] in maestro.toml"), "{out}");
    assert!(out.contains("no cap set"), "{out}");
    assert!(out.contains("$1.11") && out.contains("$1.23"), "{out}");
}

#[test]
fn cost_dashboard_with_budget_under_threshold() {
    let out = cost_view(
        vec![titled(1, SessionStatus::Completed, "Done", 1.0)],
        1.0,
        Some((5.0, 10.0)),
    );
    assert!(out.contains("Spent $1.00 of $10.00"), "{out}");
    assert!(out.contains("10%"), "{out}");
    assert!(!out.contains("← now"), "{out}");
}

#[test]
fn cost_dashboard_budget_over_90_percent() {
    let out = cost_view(
        vec![titled(1, SessionStatus::Running, "Big", 9.5)],
        9.5,
        Some((10.0, 10.0)),
    );
    assert!(out.contains("95%"), "{out}");
    let alert = out
        .lines()
        .find(|l| l.contains("$8.00"))
        .unwrap_or_default();
    assert!(
        alert.contains("← now"),
        "the crossed alert line is marked: {out}"
    );
}

#[test]
fn cost_dashboard_empty_sessions() {
    let out = cost_view(vec![], 0.0, None);
    assert!(out.contains("Nothing has run yet"), "{out}");
    assert!(!out.contains("$-"), "{out}");
}

/// The rows follow the work list (Sessions, then Finished), so the kept
/// selection is the same row in every view; it is no longer cost-sorted.
#[test]
fn cost_dashboard_multiple_sessions_sorted_by_cost() {
    let out = cost_view(
        vec![
            titled(1, SessionStatus::Completed, "Cheap task", 0.10),
            titled(2, SessionStatus::Completed, "Expensive task", 0.50),
            titled(3, SessionStatus::Running, "Medium task", 0.30),
        ],
        0.90,
        Some((5.0, 5.0)),
    );
    let at = |t| out.find(t).unwrap_or_else(|| panic!("{t}: {out}"));
    assert!(at("Medium task") < at("Cheap task"), "live before finished");
    assert!(at("Cheap task") < at("Expensive task"), "work list order");
}

#[test]
fn cost_dashboard_unpriced_session_shows_unknown_cost() {
    // #1155: an unpriced model reads `$?`, never `$0.00`, and has no cap.
    let mut unpriced = titled(
        1,
        SessionStatus::Running,
        "Routed to an unpriced model",
        0.0,
    );
    unpriced.cost_unknown = true;
    let free = titled(2, SessionStatus::Running, "Priced but free", 0.0);
    let out = cost_view(vec![unpriced, free], 0.0, Some((5.0, 50.0)));
    let row = out
        .lines()
        .find(|l| l.contains("Routed to"))
        .unwrap_or_default();
    assert!(row.contains("$?") && row.contains("no cap"), "{out}");
    let row = out
        .lines()
        .find(|l| l.contains("Priced but free"))
        .unwrap_or_default();
    assert!(row.contains("$0.00"), "{out}");
    assert!(out.contains("+ $? from 1 unpriced"), "{out}");
}
