//! Milestones' view of `work::run_plan` (#1269): the plan inputs built from
//! fetched issues, the START count, and each issue's readiness chip. The
//! planner itself is `work::run_plan`; nothing here orders issues.

use crate::orchestration::dag::{IssueMeta, IssueState, parse_blocked_by};
use crate::provider::types::{Issue, MaestroLabel};
use crate::work::conflicts::files_to_modify_from_body;
use crate::work::run_plan::{Hold, RunPlan, run_plan};
use std::collections::{HashMap, HashSet};

/// Where one issue stands in the plan. Chip words match Roadmap's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueReadiness {
    /// Open, no open blocker, no claim: it can start now.
    Ready,
    /// Waits on an open issue of this run; it starts in order.
    Waits(u64),
    /// Blocked by an open issue outside this milestone.
    BlockedOutside {
        by: u64,
        milestone: Option<u64>,
    },
    /// A session or the `maestro:in-progress` label already claims it.
    Claimed,
    /// No data to plan it (partial fetch).
    Unknown,
    Closed,
}

/// One milestone's plan plus what the rows need to explain it.
#[derive(Debug, Clone, Default)]
pub struct MilestonePlan {
    pub plan: RunPlan,
    /// Open issues of the run.
    pub open: usize,
    /// Each open issue's blockers that are still open.
    open_blockers: HashMap<u64, Vec<u64>>,
    closed: HashSet<u64>,
}

impl MilestonePlan {
    pub fn readiness(&self, n: u64) -> IssueReadiness {
        if self.closed.contains(&n) {
            return IssueReadiness::Closed;
        }
        if let Some((_, holds)) = self.plan.deferred.iter().find(|(d, _)| *d == n) {
            if holds.contains(&Hold::Claimed) {
                return IssueReadiness::Claimed;
            }
            return holds
                .iter()
                .find_map(|h| match h {
                    Hold::BlockedOutside { by, milestone } => {
                        Some(IssueReadiness::BlockedOutside {
                            by: *by,
                            milestone: *milestone,
                        })
                    }
                    _ => None,
                })
                .or_else(|| {
                    holds.iter().find_map(|h| match h {
                        Hold::WaitsOnDeferred(d) => Some(IssueReadiness::Waits(*d)),
                        _ => None,
                    })
                })
                .unwrap_or(IssueReadiness::Unknown);
        }
        match self.open_blockers.get(&n).and_then(|b| b.first()) {
            Some(&by) => IssueReadiness::Waits(by),
            None => IssueReadiness::Ready,
        }
    }

    /// START: open issues with no open blocker and no claim.
    pub fn start_count(&self) -> usize {
        self.plan
            .waves
            .iter()
            .flatten()
            .filter(|n| self.readiness(**n) == IssueReadiness::Ready)
            .count()
    }

    /// Issues the plan launches, all waves.
    pub fn runs(&self) -> usize {
        self.plan.waves.iter().map(Vec::len).sum()
    }
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

/// Plan a run of `issues` (one milestone). `known` is every other fetched
/// issue (all open milestones, the issue cache); `claimed` adds live
/// sessions to the `maestro:in-progress` label.
///
/// The fetch lists every open issue of every open milestone, so a blocker
/// that no fetch returned is taken as closed; reading it as open would hold
/// every issue whose blocker already merged.
pub fn plan_milestone<'a>(
    issues: &[Issue],
    known: impl IntoIterator<Item = &'a Issue>,
    claimed: &HashSet<u64>,
    per_session_cap_usd: f64,
) -> anyhow::Result<MilestonePlan> {
    let mut metas: HashMap<u64, IssueMeta> =
        known.into_iter().map(|i| (i.number, meta(i))).collect();
    metas.extend(issues.iter().map(|i| (i.number, meta(i))));
    let referenced: Vec<u64> = issues
        .iter()
        .flat_map(|i| metas[&i.number].blocked_by.clone())
        .collect();
    for by in referenced {
        metas.entry(by).or_insert(IssueMeta {
            number: by,
            state: IssueState::Closed,
            milestone: None,
            blocked_by: Vec::new(),
        });
    }

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
            let open_by = metas[&i.number]
                .blocked_by
                .iter()
                .copied()
                .filter(|b| metas.get(b).is_some_and(|m| m.state == IssueState::Open))
                .collect();
            (i.number, open_by)
        })
        .collect();

    Ok(MilestonePlan {
        plan: run_plan(&open, &metas, &claimed, &files, per_session_cap_usd)?,
        open: open.len(),
        open_blockers,
        closed: issues
            .iter()
            .filter(|i| !is_open(i))
            .map(|i| i.number)
            .collect(),
    })
}

#[cfg(test)]
#[path = "milestone_plan_tests.rs"]
mod tests;
