#![allow(dead_code)]

//! Level-by-level team runner for L2 dispatch (#881).
//!
//! `run_team` walks a `Scheduler`'s level DAG, spawning one session per
//! planned issue via a `TeamLauncher` trait. Per-level concurrency is
//! bounded by `Scheduler.max_parallel`. A level starts only when every
//! session of the previous level is terminal (#1246). A level-internal
//! failure (spawn error, or a session that ends killed, errored, hollow or
//! with failed gates) short-circuits the run and the downstream levels are
//! marked `skipped_due_to_upstream`.
//!
//! Replaces the placeholder fan-out in `screen_dispatch.rs::LaunchTeam`
//! that flattened every level into a single `LaunchSessions` push. See
//! `architect-blueprint.md` (R1) for the L2-vs-L1 scope decision: the
//! per-issue session here uses the team's Implementer binding; true L1
//! sub-agent routing remains a follow-up.

use crate::orchestration::scheduler::Scheduler;
use crate::session::types::SessionStatus;
use crate::state::types::IssueNumber;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{Semaphore, watch};
use uuid::Uuid;

#[async_trait]
pub trait TeamLauncher: Send + Sync {
    async fn spawn_for_issue(&self, issue: IssueNumber, agent_id: String) -> Result<Uuid, String>;

    /// Resolve once the session spawned for `issue` is terminal (#1246).
    /// `Ok` only for a clean finish; `Err` carries the failure reason.
    async fn wait_until_terminal(&self, issue: IssueNumber) -> Result<(), String>;

    /// Called when wave `wave` (0-based) starts spawning, so a UI can show
    /// which waves are still waiting.
    fn wave_started(&self, _wave: usize) {}
}

/// What a session's status means for the team gate (#1246). `None` while
/// the session can still move (Paused and Stalled included), so the next
/// wave keeps waiting. A hollow completion counts as a failure.
pub fn team_verdict(status: SessionStatus, hollow: bool) -> Option<Result<(), String>> {
    use SessionStatus::*;
    match status {
        Completed if hollow => Some(Err("session ended hollow (no real work)".to_string())),
        Completed | NeedsReview => Some(Ok(())),
        Killed => Some(Err("session was killed".to_string())),
        FailedGates => Some(Err("session failed its gates".to_string())),
        // A retry moves the session to Retrying before the feed sees it,
        // so an Errored session here has no retry left.
        Errored => Some(Err("session errored".to_string())),
        Queued | Spawning | Running | GatesRunning | Paused | Stalled | Retrying | CiFix
        | NeedsPr | ConflictFix | Interactive => None,
    }
}

/// Wait on a status feed until `verdict` decides. A closed feed is a
/// failure: nobody can report the session's end any more.
pub async fn wait_for_verdict<T>(
    mut rx: watch::Receiver<T>,
    mut verdict: impl FnMut(&T) -> Option<Result<(), String>>,
) -> Result<(), String> {
    let mut decided = None;
    let waited = rx
        .wait_for(|value| {
            decided = verdict(value);
            decided.is_some()
        })
        .await;
    match (waited, decided) {
        (Ok(_), Some(result)) => result,
        _ => Err("session feed closed before the session finished".to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueFailure {
    pub issue: IssueNumber,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct TeamOutcome {
    pub team_name: String,
    pub succeeded: Vec<IssueNumber>,
    pub failed: Vec<IssueFailure>,
    pub skipped_due_to_upstream: Vec<IssueNumber>,
    /// Why the downstream waves were skipped, e.g. `wave 1 failed: #10 ...`.
    pub skip_reason: Option<String>,
}

impl TeamOutcome {
    pub fn new(team_name: String) -> Self {
        Self {
            team_name,
            succeeded: Vec::new(),
            failed: Vec::new(),
            skipped_due_to_upstream: Vec::new(),
            skip_reason: None,
        }
    }

    /// Collapse the outcome into the `Result<(), String>` shape the
    /// wizard's `apply_launch_result` consumes. Ok iff no failures and
    /// no upstream-skipped issues; Err names the first failing issue.
    pub fn into_apply_result(self) -> Result<(), String> {
        if let Some(first) = self.failed.first() {
            return Err(format!("Issue #{} failed: {}", first.issue, first.reason));
        }
        if !self.skipped_due_to_upstream.is_empty() {
            return Err(format!(
                "{} downstream issue(s) skipped after upstream failure",
                self.skipped_due_to_upstream.len()
            ));
        }
        Ok(())
    }
}

/// Walk `scheduler.levels()` level-by-level, spawning one session per
/// issue via `launcher` with the team's resolved `agent_id` and waiting
/// until each session is terminal before the next level. Per-level
/// concurrency is capped at `scheduler.max_parallel`. On any failure in
/// a level, the remaining levels are recorded as
/// `skipped_due_to_upstream` and the walk stops.
pub async fn run_team(
    launcher: Arc<dyn TeamLauncher>,
    scheduler: Scheduler,
    app_default_agent: String,
) -> TeamOutcome {
    let agent_id = scheduler.agent_for_issue(&app_default_agent);
    let max_parallel = scheduler.max_parallel.max(1);
    let team_name = scheduler.team.name.clone();
    let sem = Arc::new(Semaphore::new(max_parallel));
    // Move the plan out instead of cloning — we own the scheduler.
    let levels: Vec<Vec<IssueNumber>> = scheduler.run.plan;
    let mut outcome = TeamOutcome::new(team_name);

    for (idx, level) in levels.iter().enumerate() {
        launcher.wave_started(idx);
        let mut handles: Vec<(IssueNumber, tokio::task::JoinHandle<Result<Uuid, String>>)> =
            Vec::with_capacity(level.len());
        for &issue in level {
            let permit = match sem.clone().acquire_owned().await {
                Ok(p) => p,
                // The semaphore Arc lives until run_team returns and is
                // never explicitly closed, so this branch is unreachable
                // in practice. Treat the failure as a per-issue defensive
                // failure rather than panic so a future refactor that
                // closes the semaphore can't crash the TUI command pump.
                Err(_) => {
                    outcome.failed.push(IssueFailure {
                        issue,
                        reason: "team_runner semaphore closed before acquire".to_string(),
                    });
                    continue;
                }
            };
            let launcher = Arc::clone(&launcher);
            let aid = agent_id.clone();
            handles.push((
                issue,
                tokio::spawn(async move {
                    // The permit spans the whole session, so `max_parallel`
                    // bounds running sessions, not only spawns.
                    let _permit = permit;
                    let session_id = launcher.spawn_for_issue(issue, aid).await?;
                    launcher.wait_until_terminal(issue).await?;
                    Ok(session_id)
                }),
            ));
        }
        let mut level_had_failure = false;
        for (issue, handle) in handles {
            match handle.await {
                Ok(Ok(_session_id)) => outcome.succeeded.push(issue),
                Ok(Err(reason)) => {
                    outcome.failed.push(IssueFailure { issue, reason });
                    level_had_failure = true;
                }
                Err(join_err) => {
                    // tokio::spawn join failures (panic, task cancelled)
                    // now record the real issue that owned the spawned
                    // task — no more synthetic `issue: 0` sentinel.
                    outcome.failed.push(IssueFailure {
                        issue,
                        reason: format!("spawn task join error: {join_err}"),
                    });
                    level_had_failure = true;
                }
            }
        }
        if level_had_failure {
            if let Some(first) = outcome.failed.first() {
                outcome.skip_reason = Some(format!(
                    "wave {} failed: #{} {}",
                    idx + 1,
                    first.issue,
                    first.reason
                ));
            }
            for downstream in &levels[idx + 1..] {
                outcome
                    .skipped_due_to_upstream
                    .extend(downstream.iter().copied());
            }
            break;
        }
    }
    outcome
}

#[cfg(test)]
#[path = "team_runner_tests.rs"]
mod tests;
