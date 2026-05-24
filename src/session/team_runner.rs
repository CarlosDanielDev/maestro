#![allow(dead_code)]

//! Level-by-level team runner for L2 dispatch (#881).
//!
//! `run_team` walks a `Scheduler`'s level DAG, spawning one session per
//! planned issue via a `TeamLauncher` trait. Per-level concurrency is
//! bounded by `Scheduler.max_parallel`. A level-internal failure
//! short-circuits the run and the downstream levels are marked
//! `skipped_due_to_upstream`.
//!
//! Replaces the placeholder fan-out in `screen_dispatch.rs::LaunchTeam`
//! that flattened every level into a single `LaunchSessions` push. See
//! `architect-blueprint.md` (R1) for the L2-vs-L1 scope decision: the
//! per-issue session here uses the team's Implementer binding; true L1
//! sub-agent routing remains a follow-up.

use crate::orchestration::scheduler::Scheduler;
use crate::state::types::IssueNumber;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

#[async_trait]
pub trait TeamLauncher: Send + Sync {
    async fn spawn_for_issue(&self, issue: IssueNumber, agent_id: String) -> Result<Uuid, String>;
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
}

impl TeamOutcome {
    pub fn new(team_name: String) -> Self {
        Self {
            team_name,
            succeeded: Vec::new(),
            failed: Vec::new(),
            skipped_due_to_upstream: Vec::new(),
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
/// issue via `launcher` with the team's resolved `agent_id`. Per-level
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
                    let _permit = permit;
                    launcher.spawn_for_issue(issue, aid).await
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
