//! Home Mission Control (#1264), drawn through the full `ui::draw` in
//! `TuiMode::Landing` with a busy fixture: three agents need you, two run,
//! one waits in the queue. Session times are left unset so the snapshots do
//! not depend on the clock (ages and elapsed times are covered by
//! `ds::fleet_table` and `ds::attention`).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use crate::session::types::SessionStatus;
use crate::tui::activity_log::{LogEntry, LogLevel};
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::{handle_key, handle_key_under_size_guard};
use crate::tui::screens::LandingScreen;
use crate::tui::screens::home::{HomeScreen, MilestoneStats, ProjectInfo, ProjectStats};
use crate::tui::snapshot_tests::no_color::no_color_snapshot_sized;
use crate::tui::snapshot_tests::{fixed_start, make_session};
use crate::tui::theme::Theme;
use crate::tui::{make_test_app, ui};

const LOG_MESSAGE: &str = "home-mission-log-1264";
/// Pool order: running agents first, so the failed-gates row is not index 0.
const FAILED_ID: u128 = 0x412;

fn agent(
    status: SessionStatus,
    issue: u64,
    title: &str,
    id: u128,
) -> crate::session::types::Session {
    let mut s = make_session(status, Some(issue));
    s.id = Uuid::from_u128(id);
    s.issue_title = Some(title.to_string());
    s.started_at = None;
    s.finished_at = None;
    s.cost_usd = 0.25 * (id % 7) as f64;
    s.context_pct = 0.1 * (id % 9) as f64;
    s
}

fn stats(milestone: Option<MilestoneStats>) -> ProjectStats {
    ProjectStats {
        loaded: true,
        issues_open: 10,
        issues_closed: 18,
        milestone_active: milestone,
        ..ProjectStats::default()
    }
}

fn v032() -> Option<MilestoneStats> {
    Some(MilestoneStats {
        title: "v0.32 TUI Redesign".to_string(),
        closed: 18,
        total: 28,
    })
}

fn local_ten_am() -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Local
        .from_local_datetime(&fixed_start().date_naive().and_hms_opt(10, 0, 0).unwrap())
        .earliest()
        .unwrap()
        .with_timezone(&chrono::Utc)
}

fn busy_app() -> App {
    busy_app_after(0)
}

/// [`busy_app`] with `runners` extra running agents enqueued first, so the
/// failed-gates session sits further down the pool (page 2 of Overview).
fn busy_app_after(runners: u128) -> App {
    let mut app = make_test_app("home-mission");
    app.show_mascot = false;
    app.show_activity_log = true;
    app.spinner_tick = 0;
    app.gh_auth_ok = true;
    app.bypass_active = false;
    app.no_splash = true;
    app.resource_monitor = Box::new(crate::system::monitor::MockResourceMonitor::new(
        22 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
    ));
    for i in 0..runners {
        let issue = 600 + i as u64;
        app.pool
            .enqueue(agent(SessionStatus::Running, issue, "Runner", 0x600 + i));
    }
    for s in [
        agent(SessionStatus::Running, 409, "Token refresh", 0x409),
        agent(SessionStatus::Running, 410, "Fleet board filters", 0x410),
        agent(SessionStatus::FailedGates, 412, "Auth refactor", FAILED_ID),
        agent(SessionStatus::NeedsReview, 398, "Plan approval", 0x398),
        agent(
            SessionStatus::NeedsPr,
            405,
            "Docs for the fleet board",
            0x405,
        ),
        agent(SessionStatus::Queued, 411, "Quota strip", 0x411),
    ] {
        app.pool.enqueue(s);
    }
    app.budget_enforcer = Some(crate::budget::BudgetEnforcer::new(5.0, 20.0, 80));
    app.total_cost = 4.82;
    let mut home = HomeScreen::new(
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: Some("carlos".to_string()),
        },
        vec![],
        vec![],
    );
    home.set_stats(stats(v032()));
    app.screen_state.home_screen = Some(home);
    app.screen_state.landing_screen = Some(LandingScreen::new());
    app.tui_mode = TuiMode::Landing;
    app.activity_log.push(LogEntry {
        timestamp: local_ten_am(),
        session_label: "S-1".to_string(),
        message: LOG_MESSAGE.to_string(),
        level: LogLevel::Info,
        tool_meta: None,
    });
    app
}

/// No open milestone: Milestone review is disabled with a reason.
fn no_milestone_app() -> App {
    let mut app = busy_app();
    if let Some(home) = app.screen_state.home_screen.as_mut() {
        home.set_stats(stats(None));
    }
    app
}

fn draw(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    t
}

fn rows(t: &Terminal<TestBackend>) -> Vec<String> {
    let buf = t.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn assert_regions(text: &str, regions: &[&str], gone: &[&str]) {
    for r in regions {
        assert!(text.contains(r), "missing {r:?}:\n{text}");
    }
    for g in gone {
        assert!(!text.contains(g), "{g:?} must not show:\n{text}");
    }
}

async fn press(app: &mut App, code: KeyCode) {
    let key = KeyEvent::new(code, KeyModifiers::NONE);
    let area = Rect::new(0, 0, 120, 36);
    if handle_key_under_size_guard(app, key, area).await.is_none() {
        handle_key(app, key).await;
    }
}

const ALL: &[&str] = &[
    "Needs you (3)",
    "Fleet (6)",
    "Actions",
    "RUN",
    "PLAN",
    "INSIGHT",
    "Budget & quota",
    "Milestone",
    "v0.32 TUI Redesign",
    "Activity Log",
    LOG_MESSAGE,
];

#[test]
fn home_wide() {
    let mut app = busy_app();
    let t = draw(&mut app, 160, 48);
    let text = rows(&t).join("\n");
    assert_regions(&text, ALL, &["Release Console", "Quit"]);
    assert!(text.contains("MODEL") && text.contains("TIME"), "{text}");
    assert_snapshot!("home_wide", t.backend());
}

#[test]
fn home_regular() {
    let mut app = busy_app();
    let t = draw(&mut app, 120, 36);
    let text = rows(&t).join("\n");
    assert_regions(&text, ALL, &["Release Console", "Quit"]);
    assert_snapshot!("home_regular", t.backend());
}

#[test]
fn home_compact() {
    let mut app = busy_app();
    let t = draw(&mut app, 80, 24);
    let text = rows(&t).join("\n");
    // Compact is one column: Actions run by key, the Milestone card drops.
    assert_regions(
        &text,
        &[
            "Needs you (3)",
            "Fleet (6)",
            "Budget & quota",
            "Activity Log",
        ],
        // #1384: no Actions panel, so the footer does not offer Tab.
        &[
            "Release Console",
            "INSIGHT",
            "v0.32 TUI Redesign",
            "MODEL",
            "Next panel",
        ],
    );
    assert_snapshot!("home_compact", t.backend());
}

#[test]
fn home_wide_no_color() {
    let mut app = busy_app();
    app.theme = Theme::no_color();
    let out = no_color_snapshot_sized(160, 48, |f, _theme| ui::draw(f, &mut app));
    assert!(out.contains("Needs you (3)"), "{out}");
    assert_snapshot!("home_wide_no_color", out);
}

#[tokio::test]
async fn home_disabled_action_inert() {
    let mut app = no_milestone_app();
    let t = draw(&mut app, 120, 36);
    let r = rows(&t);
    let row = r
        .iter()
        .find(|l| l.contains("Milestone Review"))
        .unwrap_or_else(|| panic!("no Milestone Review row:\n{}", r.join("\n")));
    assert!(row.contains('⊘'), "disabled row shows ⊘: {row:?}");
    assert!(
        r.iter().any(|l| l.contains("⊘ h no milestones yet")),
        "reason line:\n{}",
        r.join("\n")
    );
    assert_snapshot!("home_disabled_action", t.backend());

    // The letter does nothing.
    press(&mut app, KeyCode::Char('h')).await;
    assert_eq!(app.tui_mode, TuiMode::Landing, "h is inert");

    // ⏎ on the selected disabled row does nothing either.
    press(&mut app, KeyCode::Tab).await;
    let h = crate::tui::screens::home::mission::actions()
        .iter()
        .position(|a| a.key == 'h')
        .unwrap();
    for _ in 0..h {
        press(&mut app, KeyCode::Down).await;
    }
    assert_eq!(app.screen_state.mission_control.action_selected, h);
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, TuiMode::Landing, "⏎ on ⊘ is inert");

    // An enabled neighbour still runs, so the test is not passing vacuously.
    press(&mut app, KeyCode::Up).await;
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, TuiMode::Prd);
}

/// The Overview panel whose title holds `needle` is the highlighted one:
/// its top-left corner has `border_active` + BOLD (`panels::draw_single_panel`).
fn highlighted(t: &Terminal<TestBackend>, theme: &Theme, needle: &str) -> bool {
    let buf = t.backend().buffer();
    let r = rows(t);
    let y = r
        .iter()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no panel {needle:?} on screen:\n{}", r.join("\n")));
    let title_x = (0..buf.area.width)
        .find(|x| {
            let rest: String = (*x..buf.area.width)
                .map(|x| buf[(x, y as u16)].symbol())
                .collect();
            rest.starts_with(needle)
        })
        .unwrap_or(0);
    let corner = (0..title_x)
        .rev()
        .find(|x| buf[(*x, y as u16)].symbol() == "╔")
        .unwrap_or_else(|| panic!("no corner left of {needle:?}"));
    let cell = &buf[(corner, y as u16)];
    cell.fg == theme.border_active && cell.modifier.contains(ratatui::style::Modifier::BOLD)
}

/// ⏎ on the top Needs-you row (gates failed, #412) opens the session
/// Overview and highlights #412's panel, also when it is on page 2.
async fn enter_opens_overview_on(runners: u128) {
    let mut app = busy_app_after(runners);
    let t = draw(&mut app, 120, 36);
    let r = rows(&t);
    let title = r
        .iter()
        .find(|l| l.contains("Needs you (3)"))
        .unwrap_or_else(|| panic!("no Needs you panel:\n{}", r.join("\n")));
    assert!(
        title.contains('┏'),
        "Needs you is focused (Thick): {title:?}"
    );

    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, TuiMode::Overview);
    let selected = app
        .pool
        .session_at_index(app.panel_view.selected_index())
        .map(|s| s.id);
    assert_eq!(selected, Some(Uuid::from_u128(FAILED_ID)));

    let t = draw(&mut app, 120, 36);
    if runners > 0 {
        assert!(
            app.panel_view.grid_state.current_page > 0,
            "#412 is past the first page"
        );
    }
    let theme = app.theme.clone();
    assert!(
        highlighted(&t, &theme, "#412 — Auth refactor"),
        "#412 must be the highlighted panel (runners {runners}):\n{}",
        rows(&t).join("\n")
    );
    assert!(
        runners > 0 || !highlighted(&t, &theme, "#409 — Token refresh"),
        "only one panel is highlighted"
    );
}

#[tokio::test]
async fn home_enter_attention_opens_dashboard_row() {
    enter_opens_overview_on(0).await;
    // 9 more agents first: #412 is index 11, past the first page.
    enter_opens_overview_on(9).await;
}

/// Rows that carry only branding or the link chart of the old landing.
fn decoration_rows(rows: &[String]) -> usize {
    const MARKS: &[&str] = &[
        "|  \\/  |",
        "███╗   ███╗",
        "Release Console",
        "Internet",
        "Highlights",
        "Ref Trend",
    ];
    rows.iter()
        .filter(|r| MARKS.iter().any(|m| r.contains(m)))
        .count()
}

#[test]
fn home_no_decoration_after_first_run() {
    for w in [160u16, 120, 80] {
        let mut app = busy_app();
        let t = draw(&mut app, w, 60);
        let r = rows(&t);
        assert_eq!(decoration_rows(&r), 0, "{w}x60:\n{}", r.join("\n"));
        assert!(r.join("\n").contains("Needs you (3)"), "{w}x60");
    }
}
