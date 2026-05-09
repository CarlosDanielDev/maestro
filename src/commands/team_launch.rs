//! Headless launch wiring for `maestro team launch --yes`.
//!
//! Split from `commands/team.rs` to keep that file under the per-file LOC
//! cap. Provides `SchedulerRunner` (the test-injectable seam) plus
//! `launch_headless` (the loop that drives the L3 scheduler to completion
//! using a chosen runner). Production wires `ProductionSchedulerRunner`
//! which calls into `dispatch_subagent` (#663) per `NextStep::Dispatch`.

use crate::orchestration::dag::{IssueMeta, IssueState};
use crate::orchestration::loader::Loader;
use crate::orchestration::scheduler::Scheduler;
use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::{TeamInput, TeamOutput};
use crate::state::types::{IssueNumber, IssueRunState};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct LaunchOpts {
    pub preset: String,
    pub issue: Option<u64>,
    pub issues: Vec<u64>,
    pub max_parallel: usize,
}

#[derive(Debug, Default)]
pub struct LaunchOutcome {
    pub succeeded: Vec<IssueNumber>,
    pub failed: Vec<(IssueNumber, String)>,
    pub plan_levels: usize,
}

/// Async seam for headless `team launch --yes`. Production impl drives the
/// real L1 dispatch + scheduler loop; tests inject a mock that returns
/// canned `TeamOutput` per issue without touching providers.
///
/// Pattern mirrors `AgentProviderFactory::with_default_provider` from
/// `src/orchestration/dispatch.rs` (#663): a trait object lets tests
/// substitute the entire downstream-call surface in one place.
#[async_trait::async_trait]
pub trait SchedulerRunner: Send + Sync {
    async fn run_issue(
        &self,
        issue: IssueNumber,
        team: &ResolvedTeam,
    ) -> Result<TeamOutput, String>;
}

/// Production `SchedulerRunner` — drives `dispatch_subagent` (#663) for each
/// `NextStep::Dispatch` emitted by the issue's primitive machine. Returns a
/// synthetic `TeamOutput::Pr` on machine completion: real worktree + PR
/// creation lives inside the TUI today and will be extracted in a follow-up.
/// The scheduler-to-dispatch wiring itself is exercised end-to-end here.
pub(crate) struct ProductionSchedulerRunner;

#[async_trait::async_trait]
impl SchedulerRunner for ProductionSchedulerRunner {
    async fn run_issue(
        &self,
        issue: IssueNumber,
        team: &ResolvedTeam,
    ) -> Result<TeamOutput, String> {
        use crate::config::Config;
        use crate::orchestration::dispatch::{DispatchContext, dispatch_subagent};
        use crate::orchestration::primitives::{NextStep, make_machine};

        let config = Config::find_and_load().ok().map(Arc::new);
        let default_model = config
            .as_ref()
            .map(|c| c.sessions.default_model.clone())
            .unwrap_or_else(|| "opus".to_string());
        let ctx = DispatchContext::new(team.clone(), config, default_model);

        tracing::info!(
            target: "maestro::team::launch",
            issue,
            team = %team.name,
            primitive = ?team.primitive,
            "headless launch dispatching primitive machine"
        );

        let mut machine = make_machine(team.primitive, issue);
        loop {
            match machine.next() {
                NextStep::Dispatch { role, instructions } => {
                    let result = dispatch_subagent(&ctx, role, &instructions).await;
                    machine.advance(role, result);
                }
                NextStep::Done { .. } => {
                    return Ok(TeamOutput::Pr {
                        number: issue,
                        branch: format!("feat/issue-{issue}"),
                    });
                }
                NextStep::Fail { reason } => return Err(reason),
            }
        }
    }
}

fn build_metas_from_args(
    opts: &LaunchOpts,
) -> Result<(TeamInput, HashMap<IssueNumber, IssueMeta>)> {
    let issues: Vec<IssueNumber> = match (opts.issue, opts.issues.as_slice()) {
        (Some(_), &[_, ..]) => {
            return Err(anyhow!("--issue and --issues are mutually exclusive"));
        }
        (Some(n), []) => vec![n],
        (None, []) => return Err(anyhow!("--issue or --issues is required for --yes launch")),
        (None, list) => list.to_vec(),
    };

    // Synthesise minimal `IssueMeta` records — production code will replace
    // this with `gh issue view`. For now we treat the CLI input as the full
    // selection set with no implicit dependencies.
    let mut metas = HashMap::new();
    for &n in &issues {
        metas.insert(
            n,
            IssueMeta {
                number: n,
                state: IssueState::Open,
                milestone: None,
                blocked_by: Vec::new(),
            },
        );
    }
    let input = if issues.len() == 1 {
        TeamInput::Issue { number: issues[0] }
    } else {
        TeamInput::IssueSet {
            primary_milestone: None,
            issues,
        }
    };
    Ok((input, metas))
}

/// Drive a headless team launch using the supplied `SchedulerRunner`.
///
/// Tests inject a mock runner; production wires `ProductionSchedulerRunner`.
/// Returns `LaunchOutcome` summarising per-issue terminal state. Errors only
/// on plan-construction failures; per-issue failures surface through
/// `outcome.failed`.
pub async fn launch_headless(
    loader: &Loader,
    opts: LaunchOpts,
    runner: Arc<dyn SchedulerRunner>,
) -> Result<LaunchOutcome> {
    let resolved = loader.resolve()?;
    let team = resolved
        .get(&opts.preset)
        .ok_or_else(|| anyhow!("preset {:?} not found", opts.preset))?
        .clone();

    let max_parallel = opts.max_parallel.max(1);
    let (input, metas) = build_metas_from_args(&opts)?;
    let mut sched = Scheduler::from_input(team, input, metas, max_parallel)?;
    let plan_levels = sched.run.plan.len();

    let mut outcome = LaunchOutcome {
        plan_levels,
        ..Default::default()
    };

    loop {
        let ready = sched.next_ready();
        if ready.is_empty() {
            break;
        }
        for issue in ready {
            sched.run.state.insert(
                issue,
                IssueRunState::InFlight {
                    session_id: uuid::Uuid::new_v4(),
                    started_at: chrono::Utc::now(),
                },
            );
            match runner.run_issue(issue, &sched.team).await {
                Ok(output) => {
                    sched
                        .run
                        .state
                        .insert(issue, IssueRunState::Succeeded { output });
                    outcome.succeeded.push(issue);
                }
                Err(reason) => {
                    sched.run.state.insert(
                        issue,
                        IssueRunState::Failed {
                            reason: reason.clone(),
                            attempts: 1,
                        },
                    );
                    outcome.failed.push((issue, reason));
                }
            }
        }
    }

    Ok(outcome)
}

pub fn print_launch_outcome(outcome: &LaunchOutcome) {
    println!(
        "Plan: {} level(s) — {} succeeded, {} failed",
        outcome.plan_levels,
        outcome.succeeded.len(),
        outcome.failed.len()
    );
    for (issue, reason) in &outcome.failed {
        println!("  ✗ #{issue}: {reason}");
    }
}
