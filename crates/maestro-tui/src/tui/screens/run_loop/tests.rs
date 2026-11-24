//! #1287: the run loop screen. Keys go through `handle_key` and the frame
//! through `ui::draw` at 80x24 or more (the SizeGuard swallows keys below).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use super::draw::draw_view;
use super::view::RunView;
use crate::provider::types::Issue;
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::{App, TuiCommand, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::screens::SessionConfig;
use crate::tui::theme::Theme;
use crate::work::dependencies::DependencyGraph;
use crate::work::executor::{ExecutorPhase, QueueExecutor, QueueItemState};
use crate::work::queue::WorkQueue;
use crate::work::types::WorkItem;

fn base_app() -> App {
    let mut app = crate::tui::make_test_app("run-loop-1287");
    app.show_mascot = false;
    app.no_splash = true;
    let toml = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n\
                per_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
                [github]\n[notifications]\n";
    app.config = Some(toml::from_str(toml).expect("test config"));
    app
}

fn issue(number: u64) -> Issue {
    Issue {
        number,
        title: format!("Issue #{number}"),
        body: String::new(),
        labels: vec![],
        state: "open".to_string(),
        html_url: String::new(),
        milestone: None,
        assignees: vec![],
    }
}

/// A queue over `issues` with its launch configs, advanced onto the first.
pub(super) fn queue_app(issues: &[u64]) -> App {
    let mut app = base_app();
    let items: Vec<WorkItem> = issues
        .iter()
        .map(|&n| WorkItem::from_issue(issue(n)))
        .collect();
    let queue = WorkQueue::validate_selection(issues, &DependencyGraph::build(&items))
        .expect("independent queue");
    let mut exec = QueueExecutor::new(&queue);
    exec.advance().expect("advance");
    app.queue_executor = Some(exec);
    app.queue_launch_configs = Some(
        issues
            .iter()
            .map(|&n| SessionConfig {
                issue_number: Some(n),
                title: format!("Migrate surface {n}"),
                ..Default::default()
            })
            .collect(),
    );
    app.tui_mode = TuiMode::QueueExecution;
    app
}

/// Put an active session for the running item in the pool (no process).
pub(super) fn run_item(app: &mut App, issue: u64, status: SessionStatus, cost: f64) -> uuid::Uuid {
    let mut s = Session::new(
        "work".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    );
    s.status = status;
    s.cost_usd = cost;
    let id = s.id;
    app.pool.enqueue(s);
    app.pool.try_promote();
    app.queue_executor.as_mut().unwrap().set_session_id(id);
    id
}

pub(super) async fn key(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

pub(super) async fn chord(app: &mut App, c: char) {
    let mods = if c.is_ascii_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    handle_key(app, KeyEvent::new(KeyCode::Char(c), mods)).await;
}

pub(super) async fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        key(app, KeyCode::Char(c)).await;
    }
}

pub(super) fn frame(app: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    t.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", t.backend())
}

pub(super) fn launches(app: &App) -> usize {
    (app.pending_commands.iter())
        .filter(|c| matches!(c, TuiCommand::LaunchSession(_)))
        .count()
}

pub(super) fn exec(app: &App) -> &QueueExecutor {
    app.queue_executor.as_ref().unwrap()
}

#[tokio::test]
async fn run_loop_replaces_overlays() {
    let mut app = queue_app(&[5, 6]);
    run_item(&mut app, 5, SessionStatus::Running, 0.42);
    let out = frame(&mut app);
    assert!(
        out.contains("Run order"),
        "queue draws the run loop:\n{out}"
    );
    assert!(out.contains("hold queue"), "run bar controls:\n{out}");
    assert!(
        !out.contains(" Queue: "),
        "old overlay title is gone:\n{out}"
    );

    let mut app = base_app();
    let mut cont = crate::continuous::ContinuousModeState::new();
    cont.on_issue_failed(7, "Fix the parser".into(), "gate cargo test failed".into());
    app.continuous_mode = Some(cont);
    app.tui_mode = TuiMode::ContinuousPause;
    let out = frame(&mut app);
    assert!(
        out.contains("Run order"),
        "continuous draws the run loop:\n{out}"
    );
    assert!(out.contains("Retry #7"), "decision card:\n{out}");
    assert!(
        !out.contains("Session Failed — Continuous Mode"),
        "old overlay is gone:\n{out}"
    );
}

#[tokio::test]
async fn run_loop_hold() {
    let mut app = queue_app(&[5, 6]);
    let id = run_item(&mut app, 5, SessionStatus::Running, 0.42);
    chord(&mut app, 'h').await;
    assert!(exec(&app).is_held(), "h holds the queue");
    assert!(frame(&mut app).contains("resume queue"), "held run bar");

    // The running session finishes; nothing new starts.
    app.pool.get_session_mut(id).unwrap().status = SessionStatus::Completed;
    app.pool.finalize_and_teardown(id);
    app.settle_queue_item();
    assert_eq!(exec(&app).items()[0].state, QueueItemState::Succeeded);
    assert_eq!(exec(&app).phase(), ExecutorPhase::Idle, "nothing started");
    assert_eq!(launches(&app), 0);

    // `h` again resumes: the next item starts.
    chord(&mut app, 'h').await;
    assert!(!exec(&app).is_held());
    assert_eq!(
        exec(&app).phase(),
        ExecutorPhase::Running { current_index: 1 }
    );
    assert_eq!(launches(&app), 1);
}

#[tokio::test]
async fn run_loop_stop_options() {
    // Drain (default): S, type `stop`, ⏎. The running session finishes,
    // nothing new starts.
    let mut app = queue_app(&[5, 6]);
    let id = run_item(&mut app, 5, SessionStatus::Running, 0.42);
    chord(&mut app, 'S').await;
    assert!(
        app.screen_state.run_loop.stop.is_some(),
        "S opens the dialog"
    );
    key(&mut app, KeyCode::Enter).await;
    assert!(
        app.screen_state.run_loop.stop.is_some(),
        "⏎ is locked until typed"
    );
    assert!(!exec(&app).is_stopped());
    type_text(&mut app, "stop").await;
    key(&mut app, KeyCode::Enter).await;
    assert!(app.screen_state.run_loop.stop.is_none());
    assert!(exec(&app).is_stopped(), "drain stops the run");
    let s = app.pool.get_session(id).unwrap();
    assert_eq!(s.status, SessionStatus::Running, "drain keeps it running");
    assert_eq!(exec(&app).items()[1].state, QueueItemState::Pending);
    assert!(frame(&mut app).contains("stopping"), "drain shows stopping");
    chord(&mut app, 'h').await;
    chord(&mut app, 'S').await;
    assert!(!exec(&app).is_held(), "no hold once stopped");
    assert!(app.screen_state.run_loop.stop.is_none(), "no second stop");

    // Kill: S, ↓, type the running count, ⏎. The session is killed.
    let mut app = queue_app(&[5, 6]);
    let id = run_item(&mut app, 5, SessionStatus::Running, 0.42);
    chord(&mut app, 'S').await;
    key(&mut app, KeyCode::Down).await;
    type_text(&mut app, "stop").await;
    key(&mut app, KeyCode::Enter).await;
    assert!(!exec(&app).is_stopped(), "kill needs the count, not `stop`");
    for _ in 0..4 {
        key(&mut app, KeyCode::Backspace).await;
    }
    type_text(&mut app, "1").await;
    key(&mut app, KeyCode::Enter).await;
    assert!(exec(&app).is_stopped());
    let s = app.pool.get_session(id).unwrap();
    assert_eq!(s.status, SessionStatus::Killed, "kill ends it as X does");

    // Esc cancels: the run goes on.
    let mut app = queue_app(&[5, 6]);
    run_item(&mut app, 5, SessionStatus::Running, 0.42);
    chord(&mut app, 'S').await;
    key(&mut app, KeyCode::Esc).await;
    assert!(app.screen_state.run_loop.stop.is_none());
    assert!(!exec(&app).is_stopped());
    assert_eq!(app.tui_mode, TuiMode::QueueExecution);
}

/// Old single keys pick nothing: a choice is ↑↓ then ⏎.
#[tokio::test]
async fn run_loop_card_needs_pick_and_confirm() {
    let mut app = queue_app(&[5, 6]);
    run_item(&mut app, 5, SessionStatus::Errored, 0.10);
    app.queue_executor.as_mut().unwrap().mark_failure();
    chord(&mut app, 'r').await;
    chord(&mut app, 's').await;
    chord(&mut app, 'a').await;
    assert_eq!(
        exec(&app).phase(),
        ExecutorPhase::AwaitingDecision { failed_index: 0 }
    );
    // ⏎ on the default (Skip) skips and the queue goes on.
    key(&mut app, KeyCode::Enter).await;
    assert_eq!(exec(&app).items()[0].state, QueueItemState::Skipped);
    assert_eq!(
        exec(&app).phase(),
        ExecutorPhase::Running { current_index: 1 }
    );
}

fn snapshot(app: &App) -> String {
    let view = RunView::from_app(app).expect("a run");
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    t.draw(|f| draw_view(f, &view, f.area(), &Theme::dark(), false))
        .unwrap();
    format!("{}", t.backend())
}

#[test]
fn run_loop_live() {
    let mut app = queue_app(&[964, 965, 969]);
    run_item(&mut app, 964, SessionStatus::Running, 0.42);
    assert_snapshot!(snapshot(&app));
}

#[test]
fn run_loop_waiting() {
    let mut app = queue_app(&[964, 965, 969]);
    let id = run_item(&mut app, 964, SessionStatus::Completed, 0.0);
    app.pool.get_session_mut(id).unwrap().is_hollow_completion = true;
    app.queue_executor.as_mut().unwrap().mark_failure();
    assert_snapshot!(snapshot(&app));
}

#[tokio::test]
async fn run_loop_stop_dialog_fits_80_columns() {
    let mut app = queue_app(&[5, 6]);
    run_item(&mut app, 5, SessionStatus::Running, 0.42);
    chord(&mut app, 'S').await;
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    t.draw(|f| crate::tui::ui::draw(f, &mut app)).unwrap();
    let out = format!("{}", t.backend());
    for text in [
        "Stop the run?",
        "Drain: the 1 running finish",
        "Kill the 1 running now (as X does)",
        "at most $4.58 more",
        "1 not started stay open on GitHub",
        "Type stop to confirm",
    ] {
        assert!(out.contains(text), "missing {text:?}:\n{out}");
    }
}
