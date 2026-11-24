//! #1292: a recorded file conflict raises a toast. Its `p` pauses the
//! agent that does not hold the claim (never the holder) and its `⏎`
//! shows the file in Dashboard › Deps › files. Drawn at 120x36, through
//! the real key path.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::super::App;
use crate::session::transition::TransitionReason;
use crate::session::types::{SessionStatus, StreamEvent};
use crate::session::worktree::MockWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::app::TuiMode;
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

fn status(app: &App, id: uuid::Uuid) -> Option<SessionStatus> {
    app.pool.get_session(id).map(|s| s.status)
}

#[cfg(unix)]
#[tokio::test]
async fn conflict_toast_pauses_non_holder() {
    let mut app = make_app();
    let holder = running(&mut app, 407);
    let other = running(&mut app, 409);
    // The non-holder has a real process: `p` sends it SIGSTOP.
    let mut agent = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("sleep");
    app.pool.get_active_mut(other).unwrap().session.pid = Some(agent.id());

    app.process_file_claim_event(holder, &edit(407, "src/auth.rs"));
    app.process_file_claim_event(other, &edit(409, "src/auth.rs"));
    let out = draw(&mut app);
    assert!(out.contains("src/auth.rs"), "toast names the file:\n{out}");
    assert!(out.contains("p pause #409"), "toast offers p:\n{out}");

    press(&mut app, KeyCode::Char('p')).await;
    let paused = status(&app, other);
    let _ = agent.kill();
    let _ = agent.wait();
    assert_eq!(
        paused,
        Some(SessionStatus::Paused),
        "p pauses the non-holder"
    );
    assert_eq!(
        status(&app, holder),
        Some(SessionStatus::Running),
        "never the holder"
    );
    assert!(app.conflict_prompts.is_empty(), "p used the toast");
    assert_eq!(app.tui_mode, TuiMode::Dashboard, "p did not navigate");
    assert!(draw(&mut app).contains("paused #409"), "⌃Z can undo it");
}

#[tokio::test]
async fn conflict_toast_enter_shows_the_file() {
    let mut app = make_app();
    let holder = running(&mut app, 407);
    let third = running(&mut app, 410);
    app.process_file_claim_event(holder, &edit(407, "src/session.rs"));
    app.process_file_claim_event(third, &edit(410, "src/session.rs"));
    assert!(draw(&mut app).contains("⏎ show file"));
    press(&mut app, KeyCode::Enter).await;
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
        "⏎ pauses nothing"
    );
}

/// The toast keys act only while that toast is visible: once it is gone,
/// `p` and `⏎` are the screen's keys again.
#[tokio::test]
async fn conflict_toast_keys_need_the_visible_toast() {
    let mut app = make_app();
    let holder = running(&mut app, 407);
    let other = running(&mut app, 409);
    app.process_file_claim_event(holder, &edit(407, "src/auth.rs"));
    app.process_file_claim_event(other, &edit(409, "src/auth.rs"));
    // Esc dismisses the sticky toast.
    press(&mut app, KeyCode::Esc).await;
    assert!(!draw(&mut app).contains("p pause #409"));
    press(&mut app, KeyCode::Char('p')).await;
    assert_eq!(status(&app, other), Some(SessionStatus::Running));
}

/// While a text field has focus, `p` and `⏎` type: the toast keeps them.
#[tokio::test]
async fn conflict_toast_keys_never_steal_typing() {
    let mut app = make_app();
    let holder = running(&mut app, 407);
    let other = running(&mut app, 409);
    app.process_file_claim_event(holder, &edit(407, "src/auth.rs"));
    app.process_file_claim_event(other, &edit(409, "src/auth.rs"));
    app.tui_mode = TuiMode::PromptInput;
    press(&mut app, KeyCode::Char('p')).await;
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.conflict_prompts.len(), 1, "the toast kept its keys");
    assert_eq!(status(&app, other), Some(SessionStatus::Running));
}

/// A conflict toast queued behind another error toast has no keys yet:
/// `p` never acts on a toast you cannot see.
#[tokio::test]
async fn conflict_toast_queued_behind_another_toast_has_no_keys() {
    use crate::tui::ds::toast::{Severity, Toast};
    let mut app = make_app();
    let holder = running(&mut app, 407);
    let other = running(&mut app, 409);
    app.push_toast(
        Toast::new(Severity::Error, "gh pr create failed"),
        std::time::Instant::now(),
    );
    app.process_file_claim_event(holder, &edit(407, "src/auth.rs"));
    app.process_file_claim_event(other, &edit(409, "src/auth.rs"));
    assert!(
        !draw(&mut app).contains("p pause #409"),
        "queued, not shown"
    );
    press(&mut app, KeyCode::Char('p')).await;
    assert_eq!(
        app.conflict_prompts.len(),
        1,
        "p did not use the hidden toast"
    );
}
