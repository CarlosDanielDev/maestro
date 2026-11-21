//! Text that does not fit at 80 columns (#1399): it ends with `…`, wraps,
//! or drops whole parts. It is never cut mid-word, and money never prints
//! as `-0`.

use chrono::Utc;
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use super::log_drawer::{draw, log_app};
use super::make_gh_issue;
use crate::budget::BudgetEnforcer;
use crate::tui::activity_log::{ActivityLog, LogLevel};
use crate::tui::app::{App, TuiMode};
use crate::tui::marquee::MarqueeState;
use crate::tui::screens::IssueBrowserScreen;
use crate::tui::screens::ProjectStatsScreen;
use crate::tui::screens::milestone::{MilestoneEntry, MilestoneScreen};
use crate::tui::screens::prd::PrdScreen;
use crate::tui::screens::project_stats::ProjectStatsData;
use crate::tui::theme::Theme;
use crate::tui::widgets::launch_dialog::{LaunchDialog, LaunchMode};
use crate::tui::widgets::stats_bar::{StatsBar, StatsBarData};

const SIZES: [(u16, u16); 2] = [(80, 24), (80, 27)];

fn rows(t: &Terminal<TestBackend>) -> Vec<String> {
    let buf = t.backend().buffer();
    (0..buf.area.height)
        .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect())
        .collect()
}

fn row_with<'a>(rows: &'a [String], needle: &str) -> &'a str {
    rows.iter()
        .find(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle:?}:\n{}", rows.join("\n")))
}

/// `full` is shown whole, or the row ends in `…` right after `prefix`.
fn shown_whole_or_ellipsized(row: &str, prefix: &str, full: &str) {
    if row.contains(full) {
        return;
    }
    let at = row
        .find(prefix)
        .unwrap_or_else(|| panic!("{prefix:?} missing in {row:?}"));
    assert!(
        row[at..].contains('…'),
        "{full:?} is cut without …: {row:?}"
    );
}

fn app(mode: TuiMode) -> App {
    let mut app = log_app(mode);
    app.show_activity_log = false;
    app
}

#[test]
fn cost_budget_label_never_prints_negative_zero() {
    for (w, h) in SIZES {
        let mut app = app(TuiMode::CostDashboard);
        app.budget_enforcer = Some(BudgetEnforcer::new(5.0, 50.0, 80));
        // What `sync_state` stores with no sessions: an empty f64 sum.
        app.state.update_total_cost();
        app.total_cost = app.state.total_cost_usd;
        let out = rows(&draw(&mut app, w, h)).join("\n");
        assert!(out.contains("$0.00 / $50.00 (0%)"), "{w}x{h}:\n{out}");
        assert!(
            !out.contains("-0"),
            "{w}x{h} prints a negative zero:\n{out}"
        );
    }
}

#[test]
fn cost_budget_label_snaps_a_negative_zero_from_any_caller() {
    let mut app = app(TuiMode::CostDashboard);
    app.budget_enforcer = Some(BudgetEnforcer::new(5.0, 50.0, 80));
    app.total_cost = -0.0;
    let out = rows(&draw(&mut app, 80, 24)).join("\n");
    assert!(out.contains("$0.00 / $50.00 (0%)"), "{out}");
}

fn prd_app() -> App {
    let mut app = app(TuiMode::Prd);
    app.prd = Some(crate::prd::model::Prd::new());
    app.screen_state.prd_screen = Some(PrdScreen::new());
    app
}

fn stats_app() -> App {
    let mut app = app(TuiMode::ProjectStats);
    let mut screen = ProjectStatsScreen::new();
    screen.set_data(ProjectStatsData::default());
    app.screen_state.project_stats_screen = Some(screen);
    app
}

fn milestone_app() -> App {
    let mut app = app(TuiMode::MilestoneView);
    app.screen_state.milestone_screen = Some(MilestoneScreen::new(vec![MilestoneEntry {
        number: 38,
        title: "v0.38.0 — UX Spine Follow-ups".to_string(),
        description: String::new(),
        state: "open".to_string(),
        open_issues: 2,
        closed_issues: 24,
        issues: vec![],
    }]));
    app
}

fn launch_dialog_app() -> App {
    let mut app = app(TuiMode::IssueBrowser);
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1330, "Update the launch dialog copy"),
        make_gh_issue(1329, "Run the gate twice"),
    ]);
    screen.launch_dialog = Some(LaunchDialog::new(
        vec![(1330, "Update the launch dialog copy".to_string())],
        LaunchMode::Autonomous,
    ));
    app.screen_state.issue_browser_screen = Some(screen);
    app
}

fn stats_bar_rows(w: u16) -> Vec<String> {
    let theme = Theme::dark();
    let data = StatsBarData {
        loaded: true,
        repo: "CarlosDanielDev/maestro".to_string(),
        branch: "main".to_string(),
        username: Some("CarlosDanielDev".to_string()),
        issues_open: 99,
        issues_closed: 76,
        milestone_title: None,
        milestone_closed: 0,
        milestone_total: 0,
        sessions_active: 0,
        sessions_total: 0,
        minimax_forced_count: None,
    };
    let mut t = Terminal::new(TestBackend::new(w, 3)).unwrap();
    let mut marquee = MarqueeState::new();
    t.draw(|f| {
        let area = f.area();
        StatsBar::new(data, &theme).render_with_marquee(area, f.buffer_mut(), &mut marquee);
    })
    .unwrap();
    rows(&t)
}

fn activity_log_rows(w: u16) -> Vec<String> {
    let theme = Theme::dark();
    let mut log = ActivityLog::new(10);
    log.push(crate::tui::activity_log::LogEntry {
        timestamp: Utc::now(),
        session_label: "PRD".to_string(),
        message: "PRD synced — 56 milestone(s) • ingested from #1 (github:issue-1399)".to_string(),
        level: LogLevel::Info,
        tool_meta: None,
    });
    let mut t = Terminal::new(TestBackend::new(w, 3)).unwrap();
    t.draw(|f| log.draw_in_zone(f, f.area(), &theme, false, &Utc))
        .unwrap();
    rows(&t)
}

#[test]
fn no_row_is_cut_mid_word_at_80_columns() {
    for (w, h) in SIZES {
        let ctx = format!("{w}x{h}");

        let prd = rows(&draw(&mut prd_app(), w, h));
        let hint = row_with(&prd, "[Tab] pane");
        let inner = hint.trim_matches(|c: char| c == '│' || c == ' ');
        let full = "[Tab] pane  [j/k] section  [s] save  [e] export  [y] sync  [o] explore  [R] reset  [Esc] back";
        assert!(
            full.starts_with(inner)
                && (inner.len() == full.len() || full[inner.len()..].starts_with("  ")),
            "{ctx} PRD hint row cuts a hint: {hint:?}"
        );

        let mut cost = app(TuiMode::CostDashboard);
        let cost = rows(&draw(&mut cost, w, h));
        let header = row_with(&cost, "Title");
        assert!(header.contains("Status"), "{ctx} cost header: {header:?}");
        assert!(header.contains("Elapsed"), "{ctx} cost header: {header:?}");

        let stats = rows(&draw(&mut stats_app(), w, h));
        let spend = row_with(&stats, "Spend");
        shown_whole_or_ellipsized(spend, "tokens", "tokens 0 in / 0 out");

        let ms = rows(&draw(&mut milestone_app(), w, h));
        let gauge = row_with(&ms, "issues");
        shown_whole_or_ellipsized(gauge, "24/26", "24/26 issues (92%)");
        let title = row_with(&ms, "v0.38.0");
        shown_whole_or_ellipsized(title, "v0.38.0", "UX Spine Follow-ups");

        let bar = stats_bar_rows(w);
        shown_whole_or_ellipsized(&bar[1], "76", "closed (175)");

        let log = activity_log_rows(w);
        shown_whole_or_ellipsized(&log[1], "PRD synced", "(github:issue-1399)");
    }
}

#[test]
fn no_row_is_cut_mid_word_at_80_columns_snapshots() {
    assert_snapshot!("text_fit_prd_80x24", draw(&mut prd_app(), 80, 24).backend());
    let mut cost = app(TuiMode::CostDashboard);
    cost.pool.enqueue(super::make_session(
        crate::session::types::SessionStatus::NeedsReview,
        Some(1399),
    ));
    assert_snapshot!("text_fit_cost_80x24", draw(&mut cost, 80, 24).backend());
    assert_snapshot!(
        "text_fit_stats_80x24",
        draw(&mut stats_app(), 80, 24).backend()
    );
    assert_snapshot!(
        "text_fit_milestones_80x24",
        draw(&mut milestone_app(), 80, 24).backend()
    );
    assert_snapshot!(
        "text_fit_dashboard_header_80",
        stats_bar_rows(80).join("\n")
    );
}

#[test]
fn launch_dialog_options_wrap_80x24() {
    let t = draw(&mut launch_dialog_app(), 80, 24);
    let out = rows(&t).join("\n");
    // #1262: every mode states its consequence in full, and the spend,
    // the worktree and the button row fit inside the dialog.
    for mode in crate::tui::widgets::launch_dialog::LaunchMode::ALL {
        assert!(out.contains(mode.consequence()), "{mode:?} cut:\n{out}");
    }
    for text in [
        "SPEND",
        "issue-1330  branch maestro/issue-1330",
        "⏎ Launch",
        "Esc cancel",
    ] {
        assert!(out.contains(text), "{text:?} missing:\n{out}");
    }
    assert_snapshot!("launch_dialog_options_wrap_80x24", t.backend());
}

#[test]
fn overview_empty_state_names_next_action() {
    for (w, h) in SIZES {
        let mut app = app(TuiMode::Overview);
        let t = draw(&mut app, w, h);
        let out = rows(&t).join("\n");
        assert!(
            !out.contains("Waiting for sessions to start"),
            "{w}x{h}:\n{out}"
        );
        assert!(
            out.contains("i Issues") && out.contains("r Prompt"),
            "{w}x{h} names no key that starts work:\n{out}"
        );
        // Session-only footer hints are drawn dimmed.
        let buf = t.backend().buffer();
        let footer = buf.area.height - 1;
        let row = &rows(&t)[footer as usize];
        for key in ["Enter", "c Copy", "f Full"] {
            let x = row
                .find(key)
                .unwrap_or_else(|| panic!("{key:?} missing from footer {row:?}"));
            let col = row[..x].chars().count() as u16;
            let cell = &buf[(col, footer)];
            assert!(
                cell.modifier.contains(ratatui::style::Modifier::DIM),
                "{w}x{h}: {key:?} is not dimmed in {row:?}"
            );
        }
    }
}

/// #1262: `ui::draw` feeds the dialog the live `[budget]` and spend, so the
/// real screen states the cap (not the "no budget" default) and locks Enter
/// on a projected Block.
#[test]
fn launch_dialog_states_live_budget_through_ui_draw() {
    let mut app = launch_dialog_app();
    let toml = "[project]\nrepo = \"owner/repo\"\n[sessions]\n\
                [budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
                [github]\n[notifications]\n";
    app.config = Some(toml::from_str(toml).expect("test config parse"));
    app.total_cost = 12.0;
    let out = rows(&draw(&mut app, 80, 24)).join("\n");
    assert!(out.contains("$12.00 of $50.00 spent"), "{out}");
    app.total_cost = 49.8;
    let out = rows(&draw(&mut app, 80, 24)).join("\n");
    assert!(
        out.contains("⊘ locked: over the cap ($50.30 of $50.00)"),
        "{out}"
    );
    for text in ["raise [budget] total_usd in Settings", "Esc cancel"] {
        assert!(out.contains(text), "{text:?} cut:\n{out}");
    }
}
