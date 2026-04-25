//! GitHub-backed PRD syncer (#321).
//!
//! Populates `Prd::current_state` and the timeline from open/closed
//! milestones and issues. The trait is mockable; the in-memory fake is
//! used in unit tests.

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::prd::discover::{DiscoveredPrd, discover_all};
use crate::prd::ingest::{IngestedPrd, parse_markdown};
use crate::prd::model::{CurrentState, TimelineMilestone, TimelineStatus};
use crate::provider::github::client::GitHubClient;
use crate::provider::github::types::GhIssue;
use anyhow::Result;
use async_trait::async_trait;

/// What `fetch_current_state` returns. Folded into `Prd` by the App.
#[derive(Debug, Clone, PartialEq)]
pub struct PrdSyncResult {
    pub current_state: CurrentState,
    pub timeline: Vec<TimelineMilestone>,
    /// PRD content discovered + parsed from the primary source, if any.
    pub ingested: Option<IngestedPrdReport>,
    /// All PRD candidates found across every source (GitHub label /
    /// issue #1 / title-search, local `docs/PRD.md`, Azure wiki). Lets
    /// the explore UI show what's available so the user can switch.
    pub candidates: Vec<DiscoveredPrd>,
}

/// Outcome of the PRD-issue discovery + parse step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestedPrdReport {
    pub issue_number: u64,
    pub issue_title: String,
    pub source_label: &'static str,
    pub ingested: IngestedPrd,
}

/// Trait so the syncer can be faked in unit tests of higher layers.
#[async_trait]
pub trait PrdSyncer: Send + Sync {
    async fn fetch_current_state(&self) -> Result<PrdSyncResult>;
}

pub struct GitHubPrdSyncer {
    client: Box<dyn GitHubClient>,
}

impl GitHubPrdSyncer {
    pub fn new(client: Box<dyn GitHubClient>) -> Self {
        Self { client }
    }

    /// Synchronous helper exposed for the tui dispatch path that already
    /// owns a tokio runtime — saves an extra trait-object boxing.
    ///
    /// Closed-issue counts come from each milestone's built-in
    /// `closed_issues` aggregate rather than a separate `list_issues`
    /// call: `list_issues` filters by label, not by state, so passing
    /// `state:closed` as a label name produced 0 closed issues every
    /// time.
    pub async fn fetch_current_state(&self) -> Result<PrdSyncResult> {
        let (open_milestones, closed_milestones, open_issues) = tokio::join!(
            self.client.list_milestones("open"),
            self.client.list_milestones("closed"),
            self.client.list_issues(&[]),
        );

        let open_milestones = open_milestones?;
        let closed_milestones = closed_milestones?;
        let open_issues = open_issues.unwrap_or_default();

        // Sum per-milestone counts for the headline number. This includes
        // every issue assigned to ANY milestone (open or closed). Issues
        // without a milestone are still counted in `open_issues.len()` if
        // open, which is the most useful denominator for "Current State".
        let closed_issues_total: u32 = open_milestones
            .iter()
            .chain(closed_milestones.iter())
            .map(|m| m.closed_issues)
            .sum();

        let current_state = aggregate(
            &open_issues,
            closed_issues_total,
            open_milestones.len() as u32,
            closed_milestones.len() as u32,
        );

        let timeline = compute_timeline(&open_milestones, &closed_milestones);

        // Discovery + ingest of the canonical PRD across all sources
        // (GitHub label / issue #1 / title-search, local docs/PRD.md,
        // Azure wiki). Best-effort: any source that fails just doesn't
        // appear in `candidates`.
        let (candidates, ingested) = tokio::task::spawn_blocking(discover_and_parse_all)
            .await
            .unwrap_or((Vec::new(), None));

        Ok(PrdSyncResult {
            current_state,
            timeline,
            ingested,
            candidates,
        })
    }
}

fn discover_and_parse_all() -> (Vec<DiscoveredPrd>, Option<IngestedPrdReport>) {
    let candidates = discover_all();
    let primary = candidates.first().cloned();
    let ingested = primary.and_then(|c| {
        let parsed = parse_markdown(&c.body);
        if parsed.is_empty() {
            None
        } else {
            Some(IngestedPrdReport {
                issue_number: c.number,
                issue_title: c.title,
                source_label: c.source.label(),
                ingested: parsed,
            })
        }
    });
    (candidates, ingested)
}

#[async_trait]
impl PrdSyncer for GitHubPrdSyncer {
    async fn fetch_current_state(&self) -> Result<PrdSyncResult> {
        Self::fetch_current_state(self).await
    }
}

/// Pure aggregation — used by the syncer and exposed for direct testing.
pub fn aggregate(
    open_issues: &[GhIssue],
    closed_issues: u32,
    open_milestones: u32,
    closed_milestones: u32,
) -> CurrentState {
    use std::collections::HashMap;

    let mut blocker_counts: HashMap<u64, u32> = HashMap::new();
    for issue in open_issues {
        for b in issue.all_blockers() {
            *blocker_counts.entry(b).or_insert(0) += 1;
        }
    }
    let mut sorted: Vec<(u64, u32)> = blocker_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let top_blockers: Vec<u64> = sorted.into_iter().take(5).map(|(n, _)| n).collect();

    CurrentState {
        open_issues: open_issues.len() as u32,
        closed_issues,
        open_milestones,
        closed_milestones,
        top_blockers,
    }
}

/// Build a timeline from the open + closed milestones combined. Sorted
/// by milestone number ascending (creation order).
pub fn compute_timeline(
    open_milestones: &[crate::provider::github::types::GhMilestone],
    closed_milestones: &[crate::provider::github::types::GhMilestone],
) -> Vec<TimelineMilestone> {
    let mut combined: Vec<&crate::provider::github::types::GhMilestone> = open_milestones
        .iter()
        .chain(closed_milestones.iter())
        .collect();
    combined.sort_by_key(|m| m.number);

    combined
        .into_iter()
        .map(|m| {
            let total = m.open_issues + m.closed_issues;
            let progress = if total > 0 {
                f64::from(m.closed_issues) / f64::from(total)
            } else {
                0.0
            };
            let status = if m.state.eq_ignore_ascii_case("closed") {
                TimelineStatus::Completed
            } else if m.closed_issues > 0 {
                TimelineStatus::InProgress
            } else {
                TimelineStatus::Planned
            };
            let mut tm = TimelineMilestone::new(m.title.clone());
            tm.status = status;
            tm.progress = progress as f32;
            tm
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::github::types::{GhIssue, GhMilestone};

    fn issue(number: u64, blockers: &[u64]) -> GhIssue {
        // Use the format the existing `blocked_by_from_body` regex parses:
        // `blocked-by: #N` (case-insensitive). The `## Blocked By` prose
        // section in real issues is parsed elsewhere; here we exercise the
        // body-regex path.
        let body = if blockers.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = blockers
                .iter()
                .map(|n| format!("blocked-by: #{n}"))
                .collect();
            lines.join("\n")
        };
        GhIssue {
            number,
            title: format!("Issue {number}"),
            body,
            labels: vec![],
            state: "open".into(),
            html_url: format!("https://example/issues/{number}"),
            milestone: None,
            assignees: vec![],
        }
    }

    fn milestone(num: u64, title: &str, open: u32, closed: u32, state: &str) -> GhMilestone {
        GhMilestone {
            number: num,
            title: title.into(),
            description: String::new(),
            state: state.into(),
            open_issues: open,
            closed_issues: closed,
        }
    }

    #[test]
    fn aggregate_counts_top_blockers_descending() {
        let issues = vec![
            issue(1, &[10, 20]),
            issue(2, &[10]),
            issue(3, &[10, 20, 30]),
        ];
        let cs = aggregate(&issues, 5, 2, 3);
        assert_eq!(cs.open_issues, 3);
        assert_eq!(cs.closed_issues, 5);
        assert_eq!(cs.top_blockers[0], 10); // 3 references
        assert_eq!(cs.top_blockers[1], 20); // 2 references
        assert_eq!(cs.top_blockers[2], 30); // 1 reference
    }

    #[test]
    fn aggregate_with_no_blockers_returns_empty_top_blockers() {
        let issues = vec![issue(1, &[]), issue(2, &[])];
        let cs = aggregate(&issues, 0, 0, 0);
        assert!(cs.top_blockers.is_empty());
    }

    #[test]
    fn aggregate_caps_top_blockers_at_five() {
        let mut issues = Vec::new();
        for i in 0..10 {
            issues.push(issue(i, &[100 + i]));
        }
        let cs = aggregate(&issues, 0, 0, 0);
        assert_eq!(cs.top_blockers.len(), 5);
    }

    #[test]
    fn timeline_orders_by_milestone_number() {
        let open = vec![
            milestone(3, "v0.16.0", 4, 0, "open"),
            milestone(1, "v0.14.0", 0, 5, "open"),
        ];
        let closed = vec![milestone(2, "v0.15.0", 0, 8, "closed")];
        let tl = compute_timeline(&open, &closed);
        assert_eq!(tl.len(), 3);
        assert_eq!(tl[0].name, "v0.14.0");
        assert_eq!(tl[1].name, "v0.15.0");
        assert_eq!(tl[2].name, "v0.16.0");
    }

    #[test]
    fn timeline_status_completed_for_closed_milestone() {
        let closed = vec![milestone(1, "v1", 0, 3, "closed")];
        let tl = compute_timeline(&[], &closed);
        assert!(matches!(tl[0].status, TimelineStatus::Completed));
        assert!((tl[0].progress - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn timeline_status_in_progress_when_some_closed() {
        let open = vec![milestone(1, "v1", 3, 1, "open")];
        let tl = compute_timeline(&open, &[]);
        assert!(matches!(tl[0].status, TimelineStatus::InProgress));
        assert!((tl[0].progress - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn timeline_status_planned_when_zero_closed() {
        let open = vec![milestone(1, "v1", 5, 0, "open")];
        let tl = compute_timeline(&open, &[]);
        assert!(matches!(tl[0].status, TimelineStatus::Planned));
        assert!((tl[0].progress - 0.0).abs() < f32::EPSILON);
    }
}
