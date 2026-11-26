//! #1287 judge round: the run loop never spends what the user did not
//! pick. Each mode reads its own run, a pick belongs to one card, Kill
//! before a session exists means it never starts, and a held or stopped
//! queue starts no auto-retry.

use crossterm::event::KeyCode;

use super::tests::{chord, exec, frame, key, launches, queue_app, run_item, type_text};
use super::view::RunView;
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::{App, TuiMode};
use crate::work::executor::{ExecutorPhase, QueueItemState};

/// A queue waiting on #5 and a continuous pause on #7 in the same app.
fn queue_and_continuous() -> App {
    let mut app = queue_app(&[5, 6]);
    run_item(&mut app, 5, SessionStatus::Errored, 0.10);
    app.queue_executor.as_mut().unwrap().mark_failure();
    let mut cont = crate::continuous::ContinuousModeState::new();
    cont.on_issue_failed(7, "Fix the parser".into(), "gate cargo test failed".into());
    app.continuous_mode = Some(cont);
    app
}

/// #1287 judge: each mode reads its own run. A queue left in the app must
/// never draw on, or take the keys of, a continuous pause.
#[tokio::test]
async fn run_loop_mode_picks_its_own_run() {
    let mut app = queue_and_continuous();
    app.tui_mode = TuiMode::ContinuousPause;
    let out = frame(&mut app);
    assert!(out.contains("Retry #7"), "the continuous card:\n{out}");
    assert!(
        !out.contains("#5"),
        "no queue item on a continuous pause:\n{out}"
    );
    key(&mut app, KeyCode::Enter).await;
    let cont = app.continuous_mode.as_ref().unwrap();
    assert!(cont.current_failure().is_none(), "⏎ skipped #7");
    assert_eq!(cont.skipped_count, 1);
    assert_eq!(
        exec(&app).phase(),
        ExecutorPhase::AwaitingDecision { failed_index: 0 },
        "the queue was not touched"
    );

    // ↓ ⏎ retries the continuous issue, not the queue item.
    let mut app = queue_and_continuous();
    app.tui_mode = TuiMode::ContinuousPause;
    key(&mut app, KeyCode::Down).await;
    key(&mut app, KeyCode::Enter).await;
    assert!(
        app.continuous_mode
            .as_ref()
            .unwrap()
            .current_failure()
            .is_none()
    );
    assert_eq!(launches(&app), 0, "no queue retry");
    assert_eq!(exec(&app).items()[0].state, QueueItemState::Failed);

    let mut app = queue_and_continuous();
    app.tui_mode = TuiMode::QueueExecution;
    let out = frame(&mut app);
    assert!(out.contains("Retry #5") && !out.contains("#7"), "{out}");
}

#[tokio::test]
async fn run_loop_queue_card_retry() {
    let mut app = queue_app(&[5, 6]);
    run_item(&mut app, 5, SessionStatus::Errored, 0.10);
    app.queue_executor.as_mut().unwrap().mark_failure();
    key(&mut app, KeyCode::Char('j')).await;
    key(&mut app, KeyCode::Enter).await;
    assert_eq!(
        exec(&app).phase(),
        ExecutorPhase::Running { current_index: 0 }
    );
    assert_eq!(launches(&app), 1, "a new session for #5");
}

/// #1287 judge: a pick belongs to its card. A stop clears it, and another
/// issue's card opens on the free default.
#[tokio::test]
async fn run_loop_card_opens_on_default() {
    let mut app = queue_app(&[5, 6]);
    run_item(&mut app, 5, SessionStatus::Errored, 0.10);
    app.queue_executor.as_mut().unwrap().mark_failure();
    key(&mut app, KeyCode::Down).await;
    assert_eq!(app.screen_state.run_loop.choice, Some((5, 1)));

    // A new queue waiting on #9: its card opens on Skip.
    let picked = std::mem::replace(&mut app, queue_app(&[9]));
    app.screen_state.run_loop.choice = picked.screen_state.run_loop.choice;
    run_item(&mut app, 9, SessionStatus::Errored, 0.10);
    app.queue_executor.as_mut().unwrap().mark_failure();
    let card = RunView::from_app(&app).unwrap().card.unwrap();
    assert_eq!(card.selected, 0, "another card opens on the default");

    // S stop clears the pick.
    app.screen_state.run_loop.choice = Some((9, 1));
    chord(&mut app, 'S').await;
    type_text(&mut app, "stop").await;
    key(&mut app, KeyCode::Enter).await;
    assert!(exec(&app).is_stopped());
    assert_eq!(
        app.screen_state.run_loop.choice, None,
        "stop drops the pick"
    );
}

/// #1287 judge: Kill while the running item has no session yet (its issue
/// is being fetched). The dialog says so, and the session never starts.
#[tokio::test]
async fn run_loop_kill_before_the_session_starts() {
    let mut app = queue_app(&[5, 6]);
    chord(&mut app, 'S').await;
    key(&mut app, KeyCode::Down).await;
    let out = frame(&mut app);
    assert!(out.contains("Kill the 1 starting: it never runs"), "{out}");
    type_text(&mut app, "1").await;
    key(&mut app, KeyCode::Enter).await;
    assert!(exec(&app).is_stopped());

    let late = Session::new(
        "work".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(5),
        None,
    );
    let late_id = late.id;
    app.add_session(late).await.expect("add_session");
    assert!(app.pool.get_session(late_id).is_none(), "it never starts");
    let logged = |text: &str| (app.activity_log.entries().iter()).any(|e| e.message.contains(text));
    assert!(!logged("Enqueuing session"), "no false enqueue line");
    assert!(logged("not started: the run was stopped with kill"));
    assert_eq!(exec(&app).items()[0].state, QueueItemState::Failed);
    assert!(exec(&app).is_finished());
}

/// #1287 judge: a held or stopped queue starts no auto-retry of its
/// running item; without the hold the policy retries it.
#[tokio::test]
async fn run_loop_hold_blocks_auto_retry() {
    for held in [false, true] {
        let mut app = queue_app(&[5, 6]);
        let toml = "[project]\nrepo = \"owner/repo\"\n[sessions]\nmax_retries = 2\n\
                    retry_cooldown_secs = 0\n[budget]\nper_session_usd = 5.0\n\
                    total_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n";
        app.config = Some(toml::from_str(toml).expect("test config"));
        let id = run_item(&mut app, 5, SessionStatus::Errored, 0.10);
        // Fill the other slots so a retry enqueues without a process.
        for _ in 0..2 {
            app.pool.enqueue(Session::new(
                "work".into(),
                "opus".into(),
                "orchestrator".into(),
                None,
                None,
            ));
        }
        app.pool.try_promote();
        app.queue_executor.as_mut().unwrap().set_held(held);
        app.check_completions().await.expect("check_completions");
        let attempts = (app.pool.all_sessions().iter())
            .filter(|s| s.issue_number == Some(5) && s.id != id)
            .count();
        assert_eq!(attempts, usize::from(!held), "held = {held}");
    }
}

/// #1456: "Stop the run?" says what each choice does to the running
/// session's worktree. Drain deletes it once it completes; Kill deletes it
/// now (the same rows as `X`), with its uncommitted files lost.
#[tokio::test]
async fn run_loop_stop_tells_the_truth_about_worktrees() {
    let mut app = queue_app(&[5, 6]);
    let id = run_item(&mut app, 5, SessionStatus::Running, 0.42);
    let worktree = tempfile::tempdir().unwrap();
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(worktree.path())
        .status()
        .expect("git must be on PATH");
    assert!(init.success());
    std::fs::write(worktree.path().join("a.txt"), "a").unwrap();
    std::fs::write(worktree.path().join("b.txt"), "b").unwrap();
    app.pool.get_active_mut(id).unwrap().worktree_path = Some(worktree.path().to_path_buf());

    chord(&mut app, 'S').await;
    let out = frame(&mut app);
    assert!(out.contains("gone   its worktree, once it completes"), "{out}");
    assert!(out.contains("failed runs' worktrees"), "{out}");
    assert!(!out.contains("lost "), "drain loses nothing:\n{out}");

    key(&mut app, KeyCode::Down).await;
    let out = frame(&mut app);
    assert!(
        out.contains("gone   the worktree and branch maestro/issue-5"),
        "{out}"
    );
    assert!(
        out.contains("lost   2 uncommitted files, the turn in progress"),
        "{out}"
    );
}
