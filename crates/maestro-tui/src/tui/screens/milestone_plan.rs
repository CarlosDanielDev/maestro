//! Milestones' view of `work::run_plan` (#1269): the plan inputs built from
//! fetched issues, the START count, and each issue's readiness chip. The
//! planner itself is `work::run_plan`; nothing here orders issues.

use crate::orchestration::dag::{IssueMeta, IssueState};
use crate::provider::RepoProvider;
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
    /// Blocked by an issue of another repository (`owner/repo#5`); this
    /// repository's `#5` is never read in its place.
    OtherRepo,
    /// No data to plan it.
    Unknown,
}

/// Blocker ids from here up stand for another repository's issue: one per
/// blocked issue (`OTHER_REPO_MIN + issue`), never in any fetch, so
/// `run_plan` holds the issue as blocked outside.
const OTHER_REPO_MIN: u64 = u64::MAX / 2;

/// True for the id [`plan_milestone`] gives a cross-repository blocker.
pub fn is_other_repo(by: u64) -> bool {
    by >= OTHER_REPO_MIN
}

/// A `##` heading line, as `dag::parse_blocked_by` finds one (`^##\s+`):
/// the words after it, split on any whitespace (spaces, tabs, NBSP).
fn heading_words(line: &str) -> Option<Vec<&str>> {
    let rest = line.strip_prefix("##")?;
    rest.starts_with(char::is_whitespace)
        .then(|| rest.split_whitespace().collect())
}

/// True for the `## Blocked By` header, matched like
/// `dag::parse_blocked_by` (`(?im)^##\s+Blocked\s+By\s*$`).
fn is_blocked_by_header(line: &str) -> bool {
    heading_words(line).is_some_and(|w| {
        w.len() == 2 && w[0].eq_ignore_ascii_case("blocked") && w[1].eq_ignore_ascii_case("by")
    })
}

/// The `## Blocked By` refs of `body`: this repository's numbers, and the
/// refs that name another repository (`owner/repo#5`). The section and
/// "None" are read as `dag::parse_blocked_by` reads them; only the
/// cross-repository split is new.
pub(crate) fn blocked_by_refs(body: &str) -> (Vec<u64>, Vec<String>) {
    let mut lines = body.lines().skip_while(|l| !is_blocked_by_header(l));
    if lines.next().is_none() {
        return (Vec::new(), Vec::new());
    }
    let section: Vec<&str> = lines.take_while(|l| heading_words(l).is_none()).collect();
    let section = section.join("\n");
    if section.to_lowercase().contains("none") {
        return (Vec::new(), Vec::new());
    }
    let mut local = Vec::new();
    let mut other = Vec::new();
    for (i, _) in section.match_indices('#') {
        let digits: String = (section[i + 1..].chars())
            .take_while(char::is_ascii_digit)
            .collect();
        let Ok(n) = digits.parse::<u64>() else {
            continue;
        };
        // `owner/repo#5`: the word before `#` holds a `/`.
        let word: String = (section[..i].chars().rev())
            .take_while(|c| c.is_alphanumeric() || matches!(c, '/' | '.' | '-' | '_'))
            .collect();
        if word.contains('/') {
            let word: String = word.chars().rev().collect();
            other.push(format!("{word}#{n}"));
        } else {
            local.push(n);
        }
    }
    (local, other)
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
        if let Some((_, holds)) = self.plan.deferred.iter().find(|(d, _)| *d == n)
            && holds
                .iter()
                .any(|h| matches!(h, Hold::BlockedOutside { by, .. } if is_other_repo(*by)))
            && !holds.contains(&Hold::Claimed)
        {
            return IssueReadiness::OtherRepo;
        }
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

pub(crate) fn meta(issue: &Issue) -> IssueMeta {
    let (mut blocked_by, other) = blocked_by_refs(&issue.body);
    if !other.is_empty() {
        blocked_by.push(OTHER_REPO_MIN + issue.number);
    }
    IssueMeta {
        number: issue.number,
        state: if is_open(issue) {
            IssueState::Open
        } else {
            IssueState::Closed
        },
        milestone: issue.milestone,
        blocked_by,
    }
}

/// Plan a run of the open `issues` (one milestone). `known` is every other
/// fetched issue, in order: a later entry wins, so pass the looked-up
/// blockers first and the fresh milestone lists last. `issues` win over
/// all of `known`. `claimed` adds live sessions to the
/// `maestro:in-progress` label.
///
/// A blocker that neither the fetch nor [`resolve_blockers`] returned has
/// no meta, so `run_plan` holds its issue as blocked outside: its state is
/// unknown, and a guess could launch an issue whose blocker is still open.
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

/// This repository's blockers the milestone fetch did not return. The fetch
/// lists open issues only, so this is where every merged blocker ends up.
/// A cross-repository ref is never looked up here.
pub fn unfetched_blockers<'a>(fetched: impl IntoIterator<Item = &'a Issue> + Clone) -> Vec<u64> {
    let have: HashSet<u64> = fetched.clone().into_iter().map(|i| i.number).collect();
    let mut out: Vec<u64> = (fetched.into_iter())
        .filter(|i| is_open(i))
        .flat_map(|i| blocked_by_refs(&i.body).0)
        .filter(|n| !have.contains(n))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Lookups one fetch may make; the rest stay held as not fetched.
pub const LOOKUP_CAP: usize = 50;
/// Lookups in flight at once.
const LOOKUP_CONCURRENCY: usize = 4;

/// Look up the first [`LOOKUP_CAP`] blockers, at most four at a time, off
/// the draw path (the fetch task). Returns the issues found and the numbers
/// past the cap. A closed issue is satisfied, an open one holds its issue
/// as blocked outside, and a failed or skipped lookup is left out, so its
/// issue stays held as not fetched.
pub async fn resolve_blockers(
    client: &dyn RepoProvider,
    numbers: &[u64],
) -> (Vec<Issue>, Vec<u64>) {
    use futures::StreamExt;
    let (look, skipped) = numbers.split_at(numbers.len().min(LOOKUP_CAP));
    let lookups: Vec<_> = look.iter().map(|&n| client.get_issue(n)).collect();
    let results: Vec<anyhow::Result<Issue>> = futures::stream::iter(lookups)
        .buffer_unordered(LOOKUP_CONCURRENCY)
        .collect()
        .await;
    let found = results.into_iter().filter_map(Result::ok).collect();
    (found, skipped.to_vec())
}

/// The milestone fetch (#1269): every open milestone with its open issues
/// (a failed list keeps its error), then one lookup per blocker the lists
/// did not return, on every fetch. Nothing is read from the cache, so a
/// reopened blocker is seen as open.
pub async fn fetch_milestones(
    client: &dyn RepoProvider,
) -> anyhow::Result<crate::tui::app::MilestoneFetch> {
    let milestones = client.list_milestones("open").await?;
    let lists: Vec<_> = (milestones.iter())
        .map(|ms| client.list_issues_by_milestone(&ms.title))
        .collect();
    let results = futures::future::join_all(lists).await;
    let milestones: Vec<_> = (milestones.into_iter().zip(results))
        .map(|(ms, r)| (ms, r.map_err(|e| e.to_string())))
        .collect();
    let listed = milestones.iter().flat_map(|(_, r)| r.iter().flatten());
    let missing = unfetched_blockers(listed);
    let (blockers, skipped) = resolve_blockers(client, &missing).await;
    Ok(crate::tui::app::MilestoneFetch {
        milestones,
        blockers,
        skipped,
    })
}

#[cfg(test)]
#[path = "milestone_plan_tests.rs"]
mod tests;
