//! #1153 — the completion pipeline (gates, git push, GitHub calls) must
//! run OFF the render loop. The event loop draws once per iteration and
//! the header clock ticks per draw, so any blocking work inside
//! `check_completions` freezes the screen for the whole duration.
//!
//! Each test simulates the render loop's clock with a 500 ms ticker
//! task and asserts the caller is not blocked while a completion step
//! runs. Failure mode before the fix: `check_completions` runs the
//! step inline and the ticker is starved.

#![cfg(test)]

use super::App;
use crate::git::MockGitOps;
use crate::provider::github::client::mock::MockGitHubClient;
use crate::session::types::Session;
use crate::state::types::PendingIssueCompletion;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn config_with_gate_command(command: &str) -> crate::config::Config {
    let toml_str = format!(
        r#"
[project]
repo = "owner/repo"
[sessions]
[sessions.completion_gates]
enabled = true
[[sessions.completion_gates.commands]]
name = "slow_gate"
run = "{command}"
required = true
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    );
    toml::from_str(&toml_str).expect("test config parse")
}

fn config_auto_pr_disabled() -> crate::config::Config {
    let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
[gates]
enabled = false
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
auto_pr = false
[notifications]
"#;
    toml::from_str(toml_str).expect("test config parse")
}

fn make_app_with(config: crate::config::Config, git: MockGitOps) -> App {
    let app = crate::tui::make_test_app("completion-off-loop-test");
    let mut app = app.with_git_ops(Box::new(git));
    app.config = Some(config);
    app
}

fn enqueue_pending_completion(app: &mut App, issue: u64, branch: &str, worktree: PathBuf) {
    let session = Session::new(
        "p".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    );
    app.pool.enqueue(session);
    let promoted = app.pool.try_promote();
    assert!(!promoted.is_empty(), "session must be promoted to active");
    app.pending_issue_completions.push(PendingIssueCompletion {
        issue_number: issue,
        issue_numbers: vec![issue],
        success: true,
        cost_usd: 0.0,
        files_touched: vec![],
        worktree_branch: Some(branch.to_string()),
        worktree_path: Some(worktree),
        is_ci_fix: false,
    });
}

/// Spawn a ticker that emulates the render loop's per-draw clock tick
/// (header clock = now - start_time, computed once per draw).
async fn spawn_clock_ticker() -> (tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    let ticks = Arc::new(AtomicUsize::new(0));
    let t = Arc::clone(&ticks);
    let handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            t.fetch_add(1, Ordering::Relaxed);
        }
    });
    // Let the ticker settle into its first sleep so the count measures
    // only the window under test.
    tokio::time::sleep(Duration::from_millis(50)).await;
    (handle, ticks)
}

/// The issue's literal acceptance scenario: a gate command `sleep 5`
/// runs while the screen keeps redrawing — the header clock must
/// advance at least 4 times meanwhile.
#[tokio::test]
async fn issue_1153_gate_sleep5_does_not_block_clock() {
    let mut app = make_app_with(config_with_gate_command("sleep 5"), MockGitOps::new());
    let wt = tempfile::tempdir().unwrap();
    enqueue_pending_completion(&mut app, 1153, "feat/issue-1153", wt.path().to_path_buf());

    let (ticker, ticks) = spawn_clock_ticker().await;

    let t0 = Instant::now();
    app.check_completions().await.unwrap();
    let first_call = t0.elapsed();
    assert!(
        first_call < Duration::from_secs(1),
        "check_completions must not run gates on the caller thread; took {first_call:?}"
    );

    // Give the 5 s gate time to finish, then drain the outcome.
    tokio::time::sleep(Duration::from_secs(6)).await;
    app.check_completions().await.unwrap();

    let advanced = ticks.load(Ordering::Relaxed);
    assert!(
        advanced >= 4,
        "header clock must advance >=4 times during a 5 s gate; advanced {advanced} times"
    );
    assert!(
        app.activity_log
            .entries()
            .iter()
            .any(|e| e.message.contains("All required gates passed")),
        "gate outcome must be applied once the job finishes"
    );
    ticker.abort();
}

/// git push must also leave the render loop: a 2 s push advances the
/// clock ≥3 times while it runs.
#[tokio::test]
async fn issue_1153_slow_push_does_not_block_clock() {
    let ops = MockGitOps::new().with_slow_push(2);
    let mut app = make_app_with(config_auto_pr_disabled(), ops);
    let wt = tempfile::tempdir().unwrap();
    enqueue_pending_completion(&mut app, 1154, "feat/issue-1154", wt.path().to_path_buf());

    let (ticker, ticks) = spawn_clock_ticker().await;

    let t0 = Instant::now();
    app.check_completions().await.unwrap();
    let first_call = t0.elapsed();
    assert!(
        first_call < Duration::from_secs(1),
        "check_completions must not run the push on the caller thread; took {first_call:?}"
    );

    tokio::time::sleep(Duration::from_secs(3)).await;
    app.check_completions().await.unwrap();

    let advanced = ticks.load(Ordering::Relaxed);
    assert!(
        advanced >= 3,
        "header clock must advance >=3 times during a 2 s push; advanced {advanced} times"
    );
    assert!(
        app.activity_log
            .entries()
            .iter()
            .any(|e| e.message.contains("Pushed to branch feat/issue-1154")),
        "push outcome must be applied once the job finishes"
    );
    ticker.abort();
}

/// GitHub label calls must also leave the render loop (the issue's
/// frozen window included a 10 s label TLS timeout): a 2 s label call
/// advances the clock ≥3 times while it runs.
#[tokio::test]
async fn issue_1153_slow_label_call_does_not_block_clock() {
    let mock = MockGitHubClient::new().with_add_label_delay(2);
    let mut app = make_app_with(config_auto_pr_disabled(), MockGitOps::new());
    app.gh_auth_ok = true;
    app.github_client = Some(std::sync::Arc::new(mock));
    let wt = tempfile::tempdir().unwrap();
    enqueue_pending_completion(&mut app, 1155, "feat/issue-1155", wt.path().to_path_buf());

    let (ticker, ticks) = spawn_clock_ticker().await;

    let t0 = Instant::now();
    app.check_completions().await.unwrap();
    let first_call = t0.elapsed();
    assert!(
        first_call < Duration::from_secs(1),
        "check_completions must not await GitHub calls on the caller thread; took {first_call:?}"
    );

    tokio::time::sleep(Duration::from_secs(3)).await;
    app.check_completions().await.unwrap();

    let advanced = ticks.load(Ordering::Relaxed);
    assert!(
        advanced >= 3,
        "header clock must advance >=3 times during a 2 s label call; advanced {advanced} times"
    );
    ticker.abort();
}

/// `--once` and continuous mode exit or advance on `all_done()`. A job
/// still in flight (push / PR not yet applied) must keep `all_done()`
/// false, or the runtime drops the job and the PR is never opened.
#[tokio::test]
async fn issue_1153_in_flight_job_keeps_all_done_false() {
    let ops = MockGitOps::new().with_slow_push(1);
    let mut app = make_app_with(config_auto_pr_disabled(), ops);
    let wt = tempfile::tempdir().unwrap();
    enqueue_pending_completion(&mut app, 1156, "feat/issue-1156", wt.path().to_path_buf());
    let id = app.pool.find_by_issue_mut(1156).unwrap().session.id;
    app.pool.finalize_retain_worktree(id);
    assert!(app.all_done(), "precondition: the pool itself is done");

    app.check_completions().await.unwrap();
    assert!(
        !app.all_done(),
        "in-flight completion job must keep all_done() false"
    );

    tokio::time::sleep(Duration::from_millis(1500)).await;
    app.check_completions().await.unwrap();
    assert!(
        app.all_done(),
        "all_done() must turn true once the outcome is applied"
    );
}

/// A shutdown while the job runs must not lose the completion: the
/// in-flight payload stays in the persisted `pending_completions` (#514)
/// until its outcome is applied.
#[tokio::test]
async fn issue_1153_in_flight_job_stays_persisted() {
    let ops = MockGitOps::new().with_slow_push(1);
    let mut app = make_app_with(config_auto_pr_disabled(), ops);
    let wt = tempfile::tempdir().unwrap();
    enqueue_pending_completion(&mut app, 1157, "feat/issue-1157", wt.path().to_path_buf());

    app.check_completions().await.unwrap();
    assert!(
        app.state
            .pending_completions
            .iter()
            .any(|c| c.issue_number == 1157),
        "in-flight completion must stay in persisted state"
    );

    tokio::time::sleep(Duration::from_millis(1500)).await;
    app.check_completions().await.unwrap();
    assert!(
        app.state.pending_completions.is_empty(),
        "applied completion must leave persisted state"
    );
}

/// Measurement for the #1153 PR, not a regression guard: prints one
/// `close` line per completion span (`time.busy` / `time.idle`) plus how
/// long the render-loop caller was held. Run alone (global subscriber):
/// `cargo test --lib issue_1153_measure_spans -- --ignored --nocapture`
#[tokio::test]
#[ignore = "manual measurement; installs a global tracing subscriber"]
async fn issue_1153_measure_spans() {
    let _ = tracing_subscriber::fmt()
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_env_filter("maestro=info")
        .with_target(false)
        .try_init();

    let mut app = make_app_with(
        config_with_gate_command("sleep 5"),
        MockGitOps::new().with_slow_push(2),
    );
    app.gh_auth_ok = true;
    app.github_client = Some(Arc::new(MockGitHubClient::new().with_add_label_delay(2)));
    let wt = tempfile::tempdir().unwrap();
    enqueue_pending_completion(&mut app, 1153, "feat/issue-1153", wt.path().to_path_buf());

    let (ticker, ticks) = spawn_clock_ticker().await;
    let t0 = Instant::now();
    app.check_completions().await.unwrap();
    let held = t0.elapsed();
    let mut longest_tick = held;
    while !app.completions_in_flight.is_empty() {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let t = Instant::now();
        app.check_completions().await.unwrap();
        longest_tick = longest_tick.max(t.elapsed());
    }
    println!(
        "MEASURE dispatch_tick={held:?} longest_tick={longest_tick:?} \
         total={:?} clock_ticks={}",
        t0.elapsed(),
        ticks.load(Ordering::Relaxed)
    );
    ticker.abort();
}
