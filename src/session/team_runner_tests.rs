//! Tests for `session::team_runner` (#881). Lives in a sibling file so
//! the main runner stays under the 400-line cap (`scripts/check-file-size.sh`).
//! Wired in via `#[cfg(test)] #[path = "team_runner_tests.rs"] mod tests;`.

use super::*;
use crate::orchestration::dag::{IssueMeta, IssueState};
use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::team::{RoleBinding, SourceTier};
use crate::orchestration::types::{Primitive, TeamInput, TeamRole};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use tokio::time::{Duration, sleep};

/// Records every spawn call. Optionally fails specific issue numbers
/// and adds a per-spawn delay so concurrency assertions are observable.
struct FakeLauncher {
    calls: Mutex<Vec<(IssueNumber, String, Instant)>>,
    fail_issues: HashMap<IssueNumber, String>,
    delay_ms: u64,
}

impl FakeLauncher {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            fail_issues: HashMap::new(),
            delay_ms: 0,
        }
    }

    fn with_failure(mut self, issue: IssueNumber, reason: &str) -> Self {
        self.fail_issues.insert(issue, reason.to_string());
        self
    }

    fn with_delay_ms(mut self, ms: u64) -> Self {
        self.delay_ms = ms;
        self
    }

    fn calls(&self) -> Vec<(IssueNumber, String, Instant)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl TeamLauncher for FakeLauncher {
    async fn spawn_for_issue(&self, issue: IssueNumber, agent_id: String) -> Result<Uuid, String> {
        let recorded_at = Instant::now();
        self.calls
            .lock()
            .unwrap()
            .push((issue, agent_id, recorded_at));
        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }
        if let Some(reason) = self.fail_issues.get(&issue) {
            return Err(reason.clone());
        }
        Ok(Uuid::new_v4())
    }
}

fn pipeline_team(implementer: &str, reviewer: &str) -> ResolvedTeam {
    let mut bindings = HashMap::new();
    bindings.insert(
        TeamRole::Implementer,
        RoleBinding {
            agent: implementer.into(),
            mode: None,
            model_override: None,
            prompt_addendum: None,
            fallback_agent: None,
        },
    );
    bindings.insert(
        TeamRole::Reviewer,
        RoleBinding {
            agent: reviewer.into(),
            mode: None,
            model_override: None,
            prompt_addendum: None,
            fallback_agent: None,
        },
    );
    bindings.insert(
        TeamRole::Docs,
        RoleBinding {
            agent: implementer.into(),
            mode: None,
            model_override: None,
            prompt_addendum: None,
            fallback_agent: None,
        },
    );
    ResolvedTeam {
        name: "mixed-pipeline".into(),
        primitive: Primitive::Pipeline,
        min_agents: vec![implementer.into(), reviewer.into()],
        bindings,
        source_tier: SourceTier::Project,
    }
}

fn meta(n: IssueNumber, ms: Option<u64>, blocked_by: Vec<IssueNumber>) -> IssueMeta {
    IssueMeta {
        number: n,
        state: IssueState::Open,
        milestone: ms,
        blocked_by,
    }
}

/// 2-level scheduler: L0 = [10], L1 = [11, 12].
fn two_level_scheduler(max_parallel: usize) -> Scheduler {
    let mut metas = HashMap::new();
    metas.insert(10, meta(10, Some(1), vec![]));
    metas.insert(11, meta(11, Some(1), vec![10]));
    metas.insert(12, meta(12, Some(1), vec![10]));
    Scheduler::from_input(
        pipeline_team("claude", "qwen"),
        TeamInput::IssueSet {
            primary_milestone: Some(1),
            issues: vec![10, 11, 12],
        },
        metas,
        max_parallel,
    )
    .expect("scheduler build")
}

#[tokio::test]
async fn run_team_drains_level_before_next() {
    let scheduler = two_level_scheduler(3);
    let launcher = Arc::new(FakeLauncher::new().with_delay_ms(10));
    let outcome = run_team(
        Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
        scheduler,
        "app-default".to_string(),
    )
    .await;
    assert_eq!(outcome.succeeded.len(), 3);
    assert!(outcome.failed.is_empty());
    assert!(outcome.skipped_due_to_upstream.is_empty());

    let calls = launcher.calls();
    let l0_at = calls
        .iter()
        .find(|(issue, _, _)| *issue == 10)
        .map(|(_, _, t)| *t)
        .expect("L0 spawn missing");
    for (issue, _, t) in &calls {
        if *issue == 10 {
            continue;
        }
        assert!(
            *t >= l0_at + Duration::from_millis(9),
            "L1 issue {issue} spawn at {:?} should not start before L0 spawn delay completes (l0_at={:?})",
            t,
            l0_at,
        );
    }
    assert!(outcome.into_apply_result().is_ok());
}

#[tokio::test]
async fn run_team_partial_failure_names_issue() {
    let scheduler = two_level_scheduler(3);
    let launcher = Arc::new(FakeLauncher::new().with_failure(11, "simulated spawn error"));
    let outcome = run_team(
        Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
        scheduler,
        "app-default".to_string(),
    )
    .await;
    assert!(outcome.succeeded.contains(&10));
    assert_eq!(outcome.failed.len(), 1);
    assert_eq!(outcome.failed[0].issue, 11);
    assert_eq!(outcome.failed[0].reason, "simulated spawn error");
    assert!(outcome.skipped_due_to_upstream.is_empty());

    let summary = outcome.into_apply_result();
    let err = summary.expect_err("partial-failure should produce Err");
    assert_eq!(err, "Issue #11 failed: simulated spawn error");
}

#[tokio::test]
async fn run_team_skips_downstream_levels_on_failure() {
    let mut metas = HashMap::new();
    metas.insert(10, meta(10, Some(1), vec![]));
    metas.insert(11, meta(11, Some(1), vec![10]));
    metas.insert(12, meta(12, Some(1), vec![11]));
    let scheduler = Scheduler::from_input(
        pipeline_team("claude", "qwen"),
        TeamInput::IssueSet {
            primary_milestone: Some(1),
            issues: vec![10, 11, 12],
        },
        metas,
        3,
    )
    .expect("scheduler build");
    let launcher = Arc::new(FakeLauncher::new().with_failure(11, "fail at L1"));
    let outcome = run_team(
        Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
        scheduler,
        "app-default".to_string(),
    )
    .await;
    assert_eq!(outcome.succeeded, vec![10]);
    assert_eq!(outcome.failed.len(), 1);
    assert_eq!(outcome.failed[0].issue, 11);
    assert_eq!(outcome.skipped_due_to_upstream, vec![12]);
}

#[tokio::test]
async fn run_team_caps_concurrency_at_max_parallel() {
    // 5 issues at L0, max_parallel = 2, delay 30ms per spawn.
    // With cap=2 wall-clock should be ≥ ceil(5/2) * 30 = 90ms.
    let mut metas = HashMap::new();
    for n in 10..=14 {
        metas.insert(n, meta(n, Some(1), vec![]));
    }
    let scheduler = Scheduler::from_input(
        pipeline_team("claude", "qwen"),
        TeamInput::IssueSet {
            primary_milestone: Some(1),
            issues: vec![10, 11, 12, 13, 14],
        },
        metas,
        2,
    )
    .expect("scheduler build");
    let launcher = Arc::new(FakeLauncher::new().with_delay_ms(30));
    let started = Instant::now();
    let _ = run_team(
        Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
        scheduler,
        "app-default".to_string(),
    )
    .await;
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(80),
        "expected ≥80ms with semaphore cap, got {elapsed:?}",
    );
}

#[tokio::test]
async fn run_team_passes_implementer_binding_as_agent_id() {
    let scheduler = two_level_scheduler(3);
    let launcher = Arc::new(FakeLauncher::new());
    let _ = run_team(
        Arc::clone(&launcher) as Arc<dyn TeamLauncher>,
        scheduler,
        "app-default".to_string(),
    )
    .await;
    let calls = launcher.calls();
    for (_, agent_id, _) in &calls {
        assert_eq!(
            agent_id, "claude",
            "L2 session must use Implementer binding"
        );
    }
}
