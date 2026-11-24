//! Milestones' view of `work::run_plan` (#1269): the plan inputs built from
//! fetched issues, the START count, and each issue's readiness chip. The
//! planner itself is `work::run_plan`; nothing here orders issues.

use crate::orchestration::dag::{IssueMeta, IssueState, parse_blocked_by};
use crate::provider::types::{Issue, MaestroLabel};
use crate::work::conflicts::files_to_modify_from_body;
use crate::work::run_plan::{Hold, RunPlan, run_plan};
use std::collections::{HashMap, HashSet};

/// Where one open issue stands in the plan. Chip words match Roadmap's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueReadiness {
    /// In wave 1, no open blocker, no claim: `⏎` in the Run queue starts it.
    Ready,
    /// Starts after this issue: an open blocker of this run, or an issue of
    /// an earlier wave that edits the same file.
    Waits(u64),
    /// Blocked by an issue outside this run. `milestone` is `None` when no
    /// fetch returned the blocker, so its state is unknown.
    BlockedOutside { by: u64, milestone: Option<u64> },
    /// A live session or the `maestro:in-progress` label already claims it.
    Claimed,
    /// No data to plan it.
    Unknown,
}

/// One milestone's plan plus what the rows need to explain it.
#[derive(Debug, Clone, Default)]
pub struct MilestonePlan {
    pub plan: RunPlan,
    /// Open issues of the run.
    pub open: usize,
    /// Each open issue's blockers that are open in this run.
    open_blockers: HashMap<u64, Vec<u64>>,
}

impl MilestonePlan {
    pub fn readiness(&self, n: u64) -> IssueReadiness {
        if let Some((_, holds)) = self.plan.deferred.iter().find(|(d, _)| *d == n) {
            return hold_readiness(holds);
        }
        if let Some((_, after, _)) = self.plan.moved.iter().find(|(m, _, _)| *m == n) {
            return IssueReadiness::Waits(*after);
        }
        match self.open_blockers.get(&n).and_then(|b| b.first()) {
            Some(&by) => IssueReadiness::Waits(by),
            None => IssueReadiness::Ready,
        }
    }

    /// START: the issues `⏎` in the Run queue launches (wave 1).
    pub fn start_count(&self) -> usize {
        self.plan.waves.first().map_or(0, Vec::len)
    }

    /// Issues the plan launches, all waves.
    pub fn runs(&self) -> usize {
        self.plan.waves.iter().map(Vec::len).sum()
    }
}

/// The reason a held issue shows. A claim is the decision to make first.
fn hold_readiness(holds: &[Hold]) -> IssueReadiness {
    if holds.contains(&Hold::Claimed) {
        return IssueReadiness::Claimed;
    }
    holds
        .iter()
        .find_map(|h| match h {
            Hold::BlockedOutside { by, milestone } => Some(IssueReadiness::BlockedOutside {
                by: *by,
                milestone: *milestone,
            }),
            Hold::WaitsOnDeferred(d) => Some(IssueReadiness::Waits(*d)),
            _ => None,
        })
        .unwrap_or(IssueReadiness::Unknown)
}

fn is_open(issue: &Issue) -> bool {
    issue.state.eq_ignore_ascii_case("open")
}

fn meta(issue: &Issue) -> IssueMeta {
    IssueMeta {
        number: issue.number,
        state: if is_open(issue) {
            IssueState::Open
        } else {
            IssueState::Closed
        },
        milestone: issue.milestone,
        blocked_by: parse_blocked_by(&issue.body),
    }
}

/// Plan a run of the open `issues` (one milestone). `known` is every other
/// fetched issue; `claimed` adds live sessions to the `maestro:in-progress`
/// label.
///
/// A blocker that no fetch returned has no meta, so `run_plan` holds its
/// issue as blocked outside: its state is unknown, and a guess could launch
/// an issue whose blocker is still open.
pub fn plan_milestone<'a>(
    issues: &[Issue],
    known: impl IntoIterator<Item = &'a Issue>,
    claimed: &HashSet<u64>,
    per_session_cap_usd: f64,
) -> anyhow::Result<MilestonePlan> {
    let mut metas: HashMap<u64, IssueMeta> =
        known.into_iter().map(|i| (i.number, meta(i))).collect();
    metas.extend(issues.iter().map(|i| (i.number, meta(i))));

    let open_issues: Vec<&Issue> = issues.iter().filter(|i| is_open(i)).collect();
    let open: HashSet<u64> = open_issues.iter().map(|i| i.number).collect();
    let in_progress = MaestroLabel::InProgress.as_str();
    let claimed: HashSet<u64> = open_issues
        .iter()
        .filter(|i| claimed.contains(&i.number) || i.labels.iter().any(|l| l == in_progress))
        .map(|i| i.number)
        .collect();
    let files: HashMap<u64, Option<Vec<String>>> = open_issues
        .iter()
        .map(|i| (i.number, files_to_modify_from_body(&i.body)))
        .collect();
    let open_blockers = open_issues
        .iter()
        .map(|i| {
            let in_run = (metas[&i.number].blocked_by.iter().copied())
                .filter(|b| open.contains(b))
                .collect();
            (i.number, in_run)
        })
        .collect();

    Ok(MilestonePlan {
        plan: run_plan(&open, &metas, &claimed, &files, per_session_cap_usd)?,
        open: open.len(),
        open_blockers,
    })
}

#[cfg(test)]
#[path = "milestone_plan_tests.rs"]
mod tests;
