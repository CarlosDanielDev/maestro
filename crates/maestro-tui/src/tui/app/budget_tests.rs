//! #1243: the pre-spawn budget gate must run in a production-built App.
//! Every App here comes from `setup_app_from_config_with_trust` — never
//! `with_budget_projector` — so the tests cover the wiring that was missing.

use crate::budget::projector::DEFAULT_FLOOR_PCT;
use crate::config::{Config, LoadedConfig};
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::MockWorktreeManager;
use crate::setup::setup_app_from_config_with_trust;
use crate::state::store::StateStore;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tempfile::TempDir;

/// `[budget]` with a $1 cap. The agent command does not exist, so a spawn
/// that slips through fails instead of starting a real process.
fn production_app(total_usd: f64) -> (App, TempDir) {
    production_app_in(total_usd, Box::new(MockWorktreeManager::new()))
}

/// [`production_app`] with the given worktree manager.
fn production_app_in(
    total_usd: f64,
    worktrees: Box<dyn crate::session::worktree::WorktreeManager + Send>,
) -> (App, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let toml_body = format!(
        r#"[project]
repo = "owner/repo"
base_branch = "main"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = {total_usd}
alert_threshold_pct = 80
[notifications]
[agents]
default = "q"
[agents.q]
kind = "qwen"
command = "maestro-missing-agent-1243"
"#
    );
    let config: Config = toml::from_str(&toml_body).unwrap();
    let app = setup_app_from_config_with_trust(
        LoadedConfig {
            config,
            path: dir.path().join("maestro.toml"),
        },
        StateStore::new(dir.path().join("state-1243.json")),
        worktrees,
        None,
        false,
        true,
    );
    (app, dir)
}

fn fresh_session() -> Session {
    Session::new(
        "probe".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(1243),
        None,
    )
}

fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// A session in this run has spent $1.20 of a $1 cap; launching another
/// must hit the gate with Block.
async fn blocked_app() -> (App, TempDir, uuid::Uuid) {
    let (mut app, dir) = production_app(1.0);
    let spender = fresh_session();
    let spender_id = spender.id;
    app.pool.enqueue(spender);
    let _ = app.pool.try_promote();
    app.pool
        .get_active_mut(spender_id)
        .unwrap()
        .session
        .cost_usd = 1.2;
    app.sync_state();
    let session = fresh_session();
    let id = session.id;
    app.add_session(session).await.unwrap();
    (app, dir, id)
}

#[test]
fn app_wires_default_budget_projector() {
    let (app, _dir) = production_app(50.0);
    let projector = app
        .budget_projector
        .as_ref()
        .expect("production App must wire a budget projector from [budget]");
    // Same numbers as `DefaultBudgetProjector`: a fresh session projects
    // the floor, `per_session_usd × DEFAULT_FLOOR_PCT`.
    let projected = projector.projected_turn_cost(&fresh_session());
    assert!(
        (projected - 5.0 * DEFAULT_FLOOR_PCT).abs() < 1e-9,
        "projected {projected}"
    );
}

#[tokio::test]
async fn prespawn_gate_fires_with_default_projector() {
    let (mut app, _dir, id) = blocked_app().await;

    assert!(
        matches!(app.tui_mode, TuiMode::BudgetPreSpawn { session_id } if session_id == id),
        "a spawn past the cap must open the gate; got {:?}",
        app.tui_mode
    );
    let out = draw(&mut app);
    assert!(
        out.contains("Budget Alert") && out.contains("exceeds the configured budget"),
        "the gate modal must be drawn:\n{out}"
    );
}

#[tokio::test]
async fn prespawn_block_ignores_spawn_keys() {
    let (mut app, _dir, id) = blocked_app().await;
    assert!(matches!(app.tui_mode, TuiMode::BudgetPreSpawn { .. }));

    for c in ['y', 's', 'Y', 'S'] {
        handle_key(&mut app, key(c)).await;
        assert!(
            matches!(app.tui_mode, TuiMode::BudgetPreSpawn { session_id } if session_id == id),
            "`{c}` must not leave the Block gate; got {:?}",
            app.tui_mode
        );
    }
    assert!(
        !app.budget_skip_once.contains(&id),
        "Block must not arm skip-once"
    );
    let status = app.pool.get_session(id).map(|s| s.status);
    assert!(
        !matches!(
            status,
            Some(SessionStatus::Spawning | SessionStatus::Running)
        ),
        "Block must not spawn; status {status:?}"
    );
    assert!(
        !app.activity_log
            .entries()
            .iter()
            .any(|e| e.message.contains("post budget gate")),
        "no spawn attempt may be logged on Block"
    );
    let out = draw(&mut app);
    assert!(
        out.contains("[n]o") && !out.contains("[y]es") && !out.contains("[s]kip"),
        "on Block the modal must offer only [n]o:\n{out}"
    );
}

/// #1266: at 100% of `total_usd` the app keeps running, the running agent
/// is Paused (with a status change, not a bare SIGSTOP) and no new spawn
/// starts.
#[tokio::test]
async fn cap_pauses_and_blocks_spawns() {
    use crate::session::transition::TransitionReason;
    let (mut app, _dir) = production_app(1.0);
    let spender = fresh_session();
    let spender_id = spender.id;
    app.pool.enqueue(spender);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(spender_id).unwrap();
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    let mut agent = sleeper();
    managed.session.pid = Some(agent.id());
    managed.session.cost_usd = 1.2;
    app.sync_state();

    app.check_budget(spender_id);
    let _ = agent.kill();
    let _ = agent.wait();

    assert!(app.running, "the cap must not stop maestro");
    let spender = app.pool.get_session(spender_id).unwrap();
    assert_eq!(spender.status, SessionStatus::Paused);
    assert_eq!(
        spender.transition_history.last().map(|t| t.reason),
        Some(TransitionReason::BudgetCap)
    );

    let waiting = fresh_session();
    let waiting_id = waiting.id;
    app.add_session(waiting).await.unwrap();
    let status = app.pool.get_session(waiting_id).map(|s| s.status);
    assert!(
        !matches!(
            status,
            Some(SessionStatus::Spawning | SessionStatus::Running)
        ),
        "no spawn may start at the cap; status {status:?}"
    );
}

/// #1266: at the cap the Needs-you budget row says how many agents were
/// paused and how to bring them back, and the hint fits an 80-column row.
#[test]
fn cap_attention_row_names_paused() {
    use crate::session::transition::TransitionReason;
    use crate::tui::ds::attention::{QueueView, Subject, draw};
    let (mut app, _dir) = production_app(1.0);
    let spender = fresh_session();
    let spender_id = spender.id;
    app.pool.enqueue(spender);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(spender_id).unwrap();
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    let mut agent = sleeper();
    managed.session.pid = Some(agent.id());
    managed.session.cost_usd = 1.2;
    // Paused by an earlier run's quit: `r` cannot resume it, so the cap
    // row must not count it.
    let mut earlier = fresh_session();
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
        (SessionStatus::Paused, TransitionReason::UserQuit),
    ] {
        earlier.transition_to(to, why).unwrap();
    }
    app.pool.keep_saved_resumable(&[earlier]);
    app.sync_state();
    app.check_budget(spender_id);
    let _ = agent.kill();
    let _ = agent.wait();

    let items: Vec<_> = (app.attention().into_iter())
        .filter(|i| matches!(i.subject, Subject::Budget))
        .collect();
    assert_eq!(items.len(), 1, "the cap must show one budget row");
    let hint = "1 paused · raise cap, then r resumes";
    assert!(items[0].reason.contains(hint), "{:?}", items[0].reason);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let view = QueueView {
        focused: false,
        selected: 0,
        fleet_busy: true,
        now: chrono::Utc::now(),
    };
    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect::new(0, 0, 80, 5);
            let theme = crate::tui::theme::Theme::dark();
            draw(f, area, &items, view, &theme, true);
        })
        .unwrap();
    let out = format!("{}", terminal.backend());
    assert!(
        out.contains(hint),
        "the hint must be drawn whole at 80:\n{out}"
    );
}

/// A stand-in agent process the cap can SIGSTOP (killed by the test).
fn sleeper() -> std::process::Child {
    std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("sleep must be on PATH")
}

/// #1266: a session with no process to SIGSTOP (HTTP providers report no
/// pid) must not show Paused while it keeps spending: the cap cancels its
/// turn and records it Errored, which keeps its worktree for resume.
#[test]
fn cap_stops_a_session_it_cannot_pause() {
    use crate::session::transition::TransitionReason;
    let (mut app, _dir) = production_app(1.0);
    let spender = fresh_session();
    let spender_id = spender.id;
    app.pool.enqueue(spender);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(spender_id).unwrap();
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    let turn = managed.arm_turn_for_test();
    managed.session.pid = None;
    managed.session.cost_usd = 1.2;
    app.sync_state();

    app.check_budget(spender_id);

    assert!(turn.is_cancelled(), "the cap must stop the turn's spend");
    let spender = app.pool.get_session(spender_id).unwrap();
    assert_eq!(spender.status, SessionStatus::Errored, "never Paused");
    assert_eq!(
        spender.transition_history.last().map(|t| t.reason),
        Some(TransitionReason::BudgetCap)
    );
}

fn git(dir: &std::path::Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be on PATH")
        .status
        .success()
}

/// #1266 judge round 2: a session the cap stopped (no pid) must not be
/// auto-retried on the next tick: a retry ran in the main checkout and the
/// original's teardown deleted its worktree and branch. It must keep both,
/// with the worktree recorded for `maestro resume`.
#[tokio::test]
async fn cap_stop_is_not_retried_and_keeps_worktree() {
    use crate::session::transition::TransitionReason;
    let repo = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "ci@example.com"],
        &["config", "user.name", "CI"],
        &["commit", "-q", "--allow-empty", "-m", "init"],
    ] {
        assert!(git(repo.path(), args), "git {args:?}");
    }
    let worktrees = crate::session::worktree::GitWorktreeManager::new(repo.path().to_path_buf());
    let (mut app, _dir) = production_app_in(1.0, Box::new(worktrees));
    let spender = fresh_session();
    let spender_id = spender.id;
    app.pool.enqueue(spender);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(spender_id).unwrap();
    let kept = managed.worktree_path.clone().expect("a real worktree");
    std::fs::write(kept.join("work.txt"), "uncommitted").unwrap();
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }
    managed.session.pid = None; // an HTTP provider: nothing to SIGSTOP
    managed.session.cost_usd = 1.2;
    app.sync_state();

    app.check_budget(spender_id);
    app.check_completions().await.unwrap();

    assert_eq!(app.pool.total_count(), 1, "no retry may be spawned");
    let s = app.pool.get_session(spender_id).unwrap();
    assert_eq!(s.status, SessionStatus::Errored);
    assert_eq!(s.worktree_path.as_ref(), Some(&kept), "recorded for resume");
    assert!(kept.join("work.txt").exists(), "the work must survive");
    assert!(
        git(
            repo.path(),
            &["rev-parse", "--verify", "-q", "maestro/issue-1243"]
        ),
        "the branch must survive"
    );
}
