//! #1292: a new file conflict raises a toast. Its `⌃P` pauses the agent
//! that does not hold the claim (never the holder) and its `⌃O` shows the
//! file in Dashboard › Deps › files. Drawn at 120x36, through the real key
//! path.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::super::App;
use crate::session::transition::TransitionReason;
use crate::session::types::{SessionStatus, StreamEvent};
use crate::session::worktree::MockWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::app::TuiMode;
use crate::tui::ds::toast::{Severity, Toast};
use crate::tui::input_handler::handle_key;
use crate::tui::screens::dashboard::deps::{Column, Lens};
use crate::tui::screens::dashboard::usage::View;

const GRAPH_ON: &str = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n\
    per_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
    [github]\n[notifications]\n[views]\nagent_graph_enabled = true\n";

fn make_app() -> App {
    let tmp = std::env::temp_dir().join(format!(
        "maestro-conflict-toast-1292-{}.json",
        uuid::Uuid::new_v4()
    ));
    let mut app = App::new(
        StateStore::new(tmp),
        3,
        Box::new(MockWorktreeManager::new()),
        "default".into(),
        vec![],
    );
    app.config = Some(toml::from_str(GRAPH_ON).expect("test config"));
    app.tui_mode = TuiMode::Dashboard;
    app
}

/// A Running interaction session for `issue` in its mock worktree.
fn running(app: &mut App, issue: u64) -> uuid::Uuid {
    let id =
        app.pool
            .create_interaction_session(issue, false, "m".into(), "orchestrator".into(), None);
    let managed = app.pool.get_active_mut(id).expect("active");
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        let _ = managed.session.transition_to(to, why);
    }
    id
}

fn edit(issue: u64, file: &str) -> StreamEvent {
    StreamEvent::ToolUse {
        tool: "Edit".into(),
        file_path: Some(format!("/tmp/mock-worktrees/issue-{issue}/{file}")),
        command_preview: None,
        subagent_name: None,
    }
}

fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

async fn ctrl(app: &mut App, c: char) {
    handle_key(app, KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)).await;
}

/// #407 holds `src/auth.rs`, #409 edits it: one conflict toast.
fn conflict(app: &mut App) -> (uuid::Uuid, uuid::Uuid) {
    let holder = running(app, 407);
    let other = running(app, 409);
    app.process_file_claim_event(holder, &edit(407, "src/auth.rs"));
    app.process_file_claim_event(other, &edit(409, "src/auth.rs"));
    (holder, other)
}

/// A real process for `id`, so `⌃P` can SIGSTOP it. Kill it after.
fn with_process(app: &mut App, id: uuid::Uuid) -> std::process::Child {
    let agent = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("sleep");
    app.pool.get_active_mut(id).unwrap().session.pid = Some(agent.id());
    agent
}

fn status(app: &App, id: uuid::Uuid) -> Option<SessionStatus> {
    app.pool.get_session(id).map(|s| s.status)
}

#[cfg(unix)]
#[tokio::test]
async fn conflict_toast_pauses_non_holder() {
    let mut app = make_app();
    let (holder, other) = conflict(&mut app);
    let mut agent = with_process(&mut app, other);
    let out = draw(&mut app);
    assert!(out.contains("src/auth.rs"), "toast names the file:\n{out}");
    assert!(
        out.contains("#409 edits it too"),
        "toast names the agent:\n{out}"
    );
    assert!(
        out.contains("⌃P pause other agent"),
        "the footer offers ⌃P:\n{out}"
    );

    ctrl(&mut app, 'p').await;
    let paused = status(&app, other);
    let out = draw(&mut app);
    // ⌃Z undoes it: the same agent runs again (SIGCONT).
    ctrl(&mut app, 'z').await;
    let resumed = status(&app, other);
    let _ = agent.kill();
    let _ = agent.wait();
    assert_eq!(
        paused,
        Some(SessionStatus::Paused),
        "⌃P pauses the non-holder"
    );
    assert_eq!(
        status(&app, holder),
        Some(SessionStatus::Running),
        "never the holder"
    );
    assert!(app.conflict_prompts.is_empty(), "⌃P used the toast");
    assert_eq!(app.tui_mode, TuiMode::Dashboard, "⌃P did not navigate");
    assert!(out.contains("paused #409"), "undo toast:\n{out}");
    assert_eq!(resumed, Some(SessionStatus::Running), "⌃Z resumes it");
}

#[tokio::test]
async fn conflict_toast_enter_shows_the_file() {
    let mut app = make_app();
    let holder = running(&mut app, 407);
    let third = running(&mut app, 410);
    app.process_file_claim_event(holder, &edit(407, "src/session.rs"));
    app.process_file_claim_event(third, &edit(410, "src/session.rs"));
    assert!(draw(&mut app).contains("⌃O show conflict file"));
    ctrl(&mut app, 'o').await;
    let dash = &app.screen_state.dashboard;
    assert_eq!(app.tui_mode, TuiMode::Dashboard);
    assert_eq!(dash.view, View::Deps);
    assert_eq!(
        (dash.deps.lens, dash.deps.column),
        (Lens::Files, Column::Files)
    );
    assert_eq!(dash.deps.file.as_deref(), Some("src/session.rs"));
    let out = draw(&mut app);
    assert!(out.contains("holds the claim  #407"), "file card:\n{out}");
    assert_eq!(
        status(&app, third),
        Some(SessionStatus::Running),
        "⌃O pauses nothing"
    );
}

/// Once the toast is gone (Esc), its keys and its prompt are gone too.
#[tokio::test]
async fn conflict_toast_keys_need_the_visible_toast() {
    let mut app = make_app();
    let (_, other) = conflict(&mut app);
    press(&mut app, KeyCode::Esc).await;
    assert!(!draw(&mut app).contains("⌃P pause other agent"));
    assert!(app.conflict_prompts.is_empty(), "Esc forgot the prompt");
    ctrl(&mut app, 'p').await;
    assert_eq!(status(&app, other), Some(SessionStatus::Running));
}

/// With a text field focused the toast never takes a key.
#[tokio::test]
async fn conflict_toast_keys_never_steal_typing() {
    let mut app = make_app();
    let (_, other) = conflict(&mut app);
    let key = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert!(
        !app.conflict_toast_key(&key, true),
        "typing: the field gets ⌃O"
    );
    assert_eq!(app.conflict_prompts.len(), 1);
    assert_ne!(app.screen_state.dashboard.view, View::Deps);
    assert!(
        app.conflict_toast_key(&key, false),
        "not typing: ⌃O shows the file"
    );
    assert_eq!(status(&app, other), Some(SessionStatus::Running));
}

/// A modal owns its keys: under the budget pre-spawn modal the toast is
/// drawn but its chords do nothing and Enter stays the modal's; the modal
/// answers its own `n`.
#[tokio::test]
async fn conflict_toast_keys_never_act_under_a_modal() {
    let mut app = make_app();
    let (_, other) = conflict(&mut app);
    let parked = running(&mut app, 411);
    let modal = TuiMode::BudgetPreSpawn { session_id: parked };
    app.tui_mode = modal;
    let out = draw(&mut app);
    assert!(out.contains("#409 edits it too"), "the toast is up:\n{out}");
    assert!(
        !out.contains("⌃P pause other agent"),
        "no toast keys here:\n{out}"
    );
    for c in ['p', 'o'] {
        ctrl(&mut app, c).await;
        assert_eq!(app.tui_mode, modal, "⌃{c}");
    }
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.tui_mode, modal, "Enter stays in the modal");
    assert_eq!(app.conflict_prompts.len(), 1, "the toast kept its prompt");
    assert_eq!(status(&app, other), Some(SessionStatus::Running));
    press(&mut app, KeyCode::Char('n')).await;
    assert_ne!(app.tui_mode, modal, "the modal answered its own key");
}

/// The keys are shown and act only where they are scoped (the tab
/// screens): on a session screen the toast names no key and `⌃P` is not
/// the toast's.
#[tokio::test]
async fn conflict_toast_keys_only_on_tab_screens() {
    let mut app = make_app();
    let (holder, other) = conflict(&mut app);
    app.tui_mode = TuiMode::Session {
        id: holder,
        view: crate::tui::app::SessionView::Detail,
    };
    let out = draw(&mut app);
    assert!(out.contains("#409 edits it too"), "the toast is up:\n{out}");
    assert!(!out.contains("⌃P"), "no ⌃P hint off a tab:\n{out}");
    let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(
        !app.conflict_toast_key(&key, false),
        "⌃P is not the toast's here"
    );
    assert_eq!(status(&app, other), Some(SessionStatus::Running));
}

/// A conflict toast queued behind another error toast has no keys yet:
/// `⌃P` never acts on a toast you cannot see.
#[tokio::test]
async fn conflict_toast_queued_behind_another_toast_has_no_keys() {
    let mut app = make_app();
    app.push_toast(
        Toast::new(Severity::Error, "gh pr create failed"),
        std::time::Instant::now(),
    );
    let _ = conflict(&mut app);
    assert!(
        !draw(&mut app).contains("⌃P pause other agent"),
        "queued, not shown"
    );
    ctrl(&mut app, 'p').await;
    assert_eq!(
        app.conflict_prompts.len(),
        1,
        "⌃P did not use the hidden toast"
    );
}

/// The toast is for a NEW conflict: more edits of the same file by the
/// same agent only log, and never push an unseen error out of the queue.
#[tokio::test]
async fn conflict_toast_once_per_file_and_agent() {
    let mut app = make_app();
    app.push_toast(
        Toast::new(Severity::Error, "gh pr create failed"),
        std::time::Instant::now(),
    );
    let (_, other) = conflict(&mut app);
    for _ in 0..10 {
        app.process_file_claim_event(other, &edit(409, "src/auth.rs"));
    }
    assert_eq!(app.conflict_prompts.len(), 1, "one prompt");
    assert!(
        draw(&mut app).contains("gh pr create failed"),
        "not pushed out"
    );
    press(&mut app, KeyCode::Esc).await;
    assert!(
        draw(&mut app).contains("⌃P pause other agent"),
        "then the one conflict"
    );
    press(&mut app, KeyCode::Esc).await;
    assert!(!draw(&mut app).contains("edits it too"), "and no repeat");
}

/// A failed SIGSTOP never records Paused, and the toast says why.
#[cfg(unix)]
#[tokio::test]
async fn conflict_toast_failed_pause_says_why() {
    let mut app = make_app();
    let (_, other) = conflict(&mut app);
    // No process has this pid (above every pid_max), so kill(2) fails.
    app.pool.get_active_mut(other).unwrap().session.pid = Some(2_000_000_000);
    ctrl(&mut app, 'p').await;
    assert_eq!(status(&app, other), Some(SessionStatus::Running));
    let out = draw(&mut app);
    assert!(
        out.contains("#409 not paused: SIGSTOP failed"),
        "toast:\n{out}"
    );
}

/// Prompts are bounded: one per toast the queue can hold.
#[tokio::test]
async fn conflict_prompts_are_bounded() {
    let mut app = make_app();
    let holder = running(&mut app, 407);
    let other = running(&mut app, 409);
    for i in 0..12 {
        let file = format!("src/f{i}.rs");
        app.process_file_claim_event(holder, &edit(407, &file));
        app.process_file_claim_event(other, &edit(409, &file));
    }
    assert!(
        app.conflict_prompts.len() <= 6,
        "{}",
        app.conflict_prompts.len()
    );
}
