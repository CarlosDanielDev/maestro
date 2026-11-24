//! #1269: the Milestones tab (one row per milestone with START, issues by
//! plan level with readiness chips) and `R` → Run queue → launch dialog.
//! Nothing spawns before the dialog's ⏎.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use super::log_drawer::{draw, log_app};
use crate::provider::types::Issue;
use crate::tui::app::{App, TuiCommand, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::screens::milestone::{MilestoneEntry, MilestoneScreen};
use crate::tui::theme::Theme;

fn issue(number: u64, title: &str, milestone: u64, body: &str) -> Issue {
    Issue {
        number,
        title: title.to_string(),
        body: body.to_string(),
        labels: vec![],
        state: "open".to_string(),
        html_url: String::new(),
        milestone: Some(milestone),
        assignees: vec![],
    }
}

fn body(blocked_by: &[u64], files: &[&str]) -> String {
    let by: Vec<String> = blocked_by.iter().map(|n| format!("- #{n}")).collect();
    let by = if by.is_empty() {
        "- None".to_string()
    } else {
        by.join("\n")
    };
    let files: Vec<String> = files.iter().map(|f| format!("- `{f}`")).collect();
    let mut out = format!("## Overview\n\nMigrate one surface.\n\n## Blocked By\n\n{by}\n");
    if !files.is_empty() {
        out.push_str(&format!("\n## Files to Modify\n\n{}\n", files.join("\n")));
    }
    out
}

/// v0.33.0: two ready (#964, #965), one moved a wave down for a shared
/// file (#969), one waiting in order (#966), one claimed (#967), one
/// blocked by #831 in v0.32.0 (#968).
fn milestones() -> Vec<MilestoneEntry> {
    let mut claimed = issue(
        967,
        "Migrate the PRD fields",
        62,
        &body(&[], &["src/prd.rs"]),
    );
    claimed.labels = vec!["maestro:in-progress".into()];
    let v33 = vec![
        issue(
            964,
            "Migrate the Issue wizard to Input",
            62,
            &body(&[], &["src/wizard.rs"]),
        ),
        issue(965, "Spike: input history", 62, &body(&[], &[])),
        issue(
            966,
            "Migrate the Settings forms",
            62,
            &body(&[964], &["src/settings.rs"]),
        ),
        claimed,
        issue(
            968,
            "Delete TextAreaField",
            62,
            &body(&[831], &["src/field.rs"]),
        ),
        issue(
            969,
            "Migrate the Roadmap filter",
            62,
            &body(&[], &["src/wizard.rs"]),
        ),
    ];
    let entry = |number, title: &str, open, closed, issues| MilestoneEntry {
        number,
        title: title.to_string(),
        description: String::new(),
        state: "open".to_string(),
        open_issues: open,
        closed_issues: closed,
        issues,
    };
    vec![
        entry(
            61,
            "v0.32.0 Polish",
            1,
            9,
            vec![issue(831, "Fix the toast", 61, "")],
        ),
        entry(62, "v0.33.0 Input Standardization", 6, 2, v33),
        entry(63, "Backlog", 0, 0, vec![]),
    ]
}

fn screen() -> MilestoneScreen {
    let mut s = MilestoneScreen::new(Vec::new());
    s.set_milestones(milestones(), 5.0);
    s.selected = 1;
    s
}

fn draw_screen(s: &mut MilestoneScreen, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    let theme = Theme::dark();
    t.draw(|f| s.draw_impl(f, f.area(), &theme, false)).unwrap();
    t
}

#[test]
fn milestones_wide() {
    assert_snapshot!(draw_screen(&mut screen(), 160, 30).backend());
}

#[test]
fn milestones_compact() {
    assert_snapshot!(draw_screen(&mut screen(), 80, 24).backend());
}

#[test]
fn milestones_rows_show_start_count() {
    let s = screen();
    assert_eq!(s.start_count(0), Some(1), "#831 is ready");
    assert_eq!(s.start_count(1), Some(3), "#964 #965 #969");
    assert_eq!(s.start_count(2), Some(0));
}

fn milestone_app() -> App {
    let mut app = log_app(TuiMode::MilestoneView);
    app.config = Some(
        toml::from_str(concat!(
            "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
            "[sessions]\n",
            "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
            "[github]\n",
            "[notifications]\nslack_webhook_url = \"\"\n",
        ))
        .expect("config parses"),
    );
    app.screen_state.milestone_screen = Some(screen());
    let _ = draw(&mut app, 80, 24);
    app
}

async fn key(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
    let _ = draw(app, 80, 24);
}

fn spawned(app: &App) -> bool {
    app.pool.total_count() > 0
        || app.queue_executor.is_some()
        || app.pending_commands.iter().any(|c| {
            matches!(
                c,
                TuiCommand::LaunchSession(_)
                    | TuiCommand::LaunchSessions(_)
                    | TuiCommand::LaunchUnifiedSession(_)
            )
        })
}

#[allow(non_snake_case)]
#[tokio::test]
async fn milestones_R_opens_run_queue() {
    let mut app = milestone_app();
    key(&mut app, KeyCode::Char('R')).await;
    assert_eq!(app.tui_mode, TuiMode::QueueConfirmation);
    let screen = app.screen_state.queue_confirmation_screen.as_ref().unwrap();
    let run = screen
        .run_queue()
        .expect("the Run queue, not the old review");
    assert_eq!(
        run.summary(),
        "6 open -> 4 run in 2 waves · 1 needs a decision · 1 deferred"
    );
    assert!(screen.launch_dialog().is_none(), "the plan shows first");
    assert!(!spawned(&app), "R never spawns");
}

#[tokio::test]
async fn run_queue_enter_opens_launch_dialog() {
    let mut app = milestone_app();
    key(&mut app, KeyCode::Char('R')).await;
    key(&mut app, KeyCode::Enter).await;
    let screen = app.screen_state.queue_confirmation_screen.as_ref().unwrap();
    let dialog = screen.launch_dialog().expect("⏎ opens the launch dialog");
    let numbers: Vec<u64> = dialog.selected_issues.iter().map(|(n, _)| *n).collect();
    assert_eq!(numbers, vec![964, 965], "wave 1 of the plan");
    assert_eq!(dialog.sessions(), 2);
    assert!(!spawned(&app), "the dialog is the confirm; nothing ran yet");
    // Esc goes back to the plan, still nothing launched.
    key(&mut app, KeyCode::Esc).await;
    let screen = app.screen_state.queue_confirmation_screen.as_ref().unwrap();
    assert!(screen.launch_dialog().is_none());
    assert_eq!(app.tui_mode, TuiMode::QueueConfirmation);
    assert!(!spawned(&app));
}

#[tokio::test]
async fn run_queue_dialog_enter_queues_wave_one() {
    let mut app = milestone_app();
    key(&mut app, KeyCode::Char('R')).await;
    key(&mut app, KeyCode::Enter).await;
    key(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, TuiMode::QueueExecution);
    let configs = app.queue_launch_configs.as_ref().expect("queued");
    let numbers: Vec<Option<u64>> = configs.iter().map(|c| c.issue_number).collect();
    assert_eq!(numbers, vec![Some(964), Some(965)]);
}

#[test]
fn run_queue() {
    use crate::tui::screens::milestone_plan::plan_milestone;
    use crate::tui::screens::run_queue::RunQueue;
    let all = milestones();
    let known = all.iter().flat_map(|m| m.issues.iter());
    let entry = &all[1];
    let p = plan_milestone(&entry.issues, known, &Default::default(), 5.0).unwrap();
    let titles = entry
        .issues
        .iter()
        .map(|i| (i.number, i.title.clone()))
        .collect();
    let run = RunQueue::new(entry.title.clone(), p.open, p.plan, titles);
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();
    t.draw(|f| run.draw(f, f.area(), &theme, false)).unwrap();
    assert_snapshot!(t.backend());
}

#[test]
fn milestone_fetch_fills_issue_cache() {
    let mut app = log_app(TuiMode::MilestoneView);
    app.screen_state.milestone_screen = Some(MilestoneScreen::new(Vec::new()));
    let fetched = milestones()
        .into_iter()
        .map(|m| {
            let ms = crate::provider::types::Milestone {
                number: m.number,
                title: m.title,
                description: m.description,
                state: m.state,
                open_issues: m.open_issues,
                closed_issues: m.closed_issues,
            };
            (ms, m.issues)
        })
        .collect();
    app.handle_data_event(crate::tui::app::TuiDataEvent::Milestones(Ok(fetched)));
    assert!(app.state.issue_cache.contains_key(&964));
    assert!(app.state.issue_cache.contains_key(&831));
    let screen = app.screen_state.milestone_screen.as_ref().unwrap();
    assert_eq!(screen.start_count(1), Some(3), "planned on fetch");
}
