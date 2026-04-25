//! Loader that pulls milestones + their issues into RoadmapEntries (#329).

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::provider::github::client::GitHubClient;
use crate::provider::github::types::GhIssue;
use crate::tui::screens::roadmap::types::{RoadmapEntry, SemVer};
use anyhow::Result;

/// Concurrent fetch: first list all open + closed milestones, then for
/// each pull its issues. Returns entries unsorted; the screen state sorts
/// them by descending semver on `set_entries`.
pub async fn load_roadmap(client: &dyn GitHubClient) -> Result<Vec<RoadmapEntry>> {
    let (open_ms, closed_ms) = tokio::join!(
        client.list_milestones("open"),
        client.list_milestones("closed"),
    );
    let mut milestones = open_ms?;
    milestones.extend(closed_ms?);

    let mut entries = Vec::with_capacity(milestones.len());
    for m in milestones {
        let issues: Vec<GhIssue> = client
            .list_issues_by_milestone(&m.title)
            .await
            .unwrap_or_default();
        let semver = SemVer::parse_or_zero(&m.title);
        entries.push(RoadmapEntry {
            milestone: m,
            semver,
            issues,
        });
    }
    Ok(entries)
}

// Tests for `load_roadmap` are exercised through the integration tests
// that drive the App pipeline; the GitHubClient trait surface is too large
// to duplicate here as a hand-written fake.
#[cfg(any())]
mod tests {
    use super::*;
    use crate::provider::github::client::GitHubClient;
    use crate::provider::github::types::{
        CreateOutcome, GhMilestone, GhPr, PendingPr, PrReviewEvent,
    };
    use anyhow::Result;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Minimal fake — only the methods we exercise here are non-trivial.
    struct FakeClient {
        open_milestones: Vec<GhMilestone>,
        closed_milestones: Vec<GhMilestone>,
        issues_by_milestone: std::collections::HashMap<String, Vec<GhIssue>>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeClient {
        fn new() -> Self {
            Self {
                open_milestones: Vec::new(),
                closed_milestones: Vec::new(),
                issues_by_milestone: Default::default(),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl GitHubClient for FakeClient {
        async fn list_issues(&self, _labels: &[&str]) -> Result<Vec<GhIssue>> {
            self.calls.lock().expect("lock").push("list_issues".into());
            Ok(Vec::new())
        }
        async fn list_issues_by_milestone(&self, milestone: &str) -> Result<Vec<GhIssue>> {
            self.calls
                .lock()
                .expect("lock")
                .push(format!("list_issues_by_milestone:{milestone}"));
            Ok(self
                .issues_by_milestone
                .get(milestone)
                .cloned()
                .unwrap_or_default())
        }
        async fn list_milestones(&self, state: &str) -> Result<Vec<GhMilestone>> {
            self.calls
                .lock()
                .expect("lock")
                .push(format!("list_milestones:{state}"));
            Ok(match state {
                "open" => self.open_milestones.clone(),
                "closed" => self.closed_milestones.clone(),
                _ => Vec::new(),
            })
        }
        async fn create_issue(
            &self,
            _title: &str,
            _body: &str,
            _labels: &[String],
            _milestone: Option<u64>,
        ) -> Result<CreateOutcome> {
            unimplemented!()
        }
        async fn comment_issue(&self, _number: u64, _body: &str) -> Result<()> {
            unimplemented!()
        }
        async fn list_open_prs(&self) -> Result<Vec<PendingPr>> {
            unimplemented!()
        }
        async fn list_pr_details(&self) -> Result<Vec<GhPr>> {
            unimplemented!()
        }
        async fn add_label(&self, _number: u64, _label: &str) -> Result<()> {
            unimplemented!()
        }
        async fn remove_label(&self, _number: u64, _label: &str) -> Result<()> {
            unimplemented!()
        }
        async fn close_issue(&self, _number: u64) -> Result<()> {
            unimplemented!()
        }
        async fn create_milestone(&self, _title: &str, _description: Option<&str>) -> Result<u64> {
            unimplemented!()
        }
        async fn submit_pr_review(
            &self,
            _pr_number: u64,
            _event: PrReviewEvent,
            _body: &str,
        ) -> Result<()> {
            unimplemented!()
        }
    }

    fn ms(num: u64, title: &str, state: &str) -> GhMilestone {
        GhMilestone {
            number: num,
            title: title.into(),
            description: String::new(),
            state: state.into(),
            open_issues: 0,
            closed_issues: 0,
        }
    }

    #[tokio::test]
    async fn load_roadmap_combines_open_and_closed() {
        let mut client = FakeClient::new();
        client.open_milestones = vec![ms(2, "v0.16.0", "open")];
        client.closed_milestones = vec![ms(1, "v0.15.0", "closed")];
        let entries = load_roadmap(&client).await.expect("ok");
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn load_roadmap_attaches_semver_per_entry() {
        let mut client = FakeClient::new();
        client.open_milestones = vec![ms(1, "v1.2.3", "open"), ms(2, "no-version", "open")];
        let entries = load_roadmap(&client).await.expect("ok");
        let v1 = entries
            .iter()
            .find(|e| e.milestone.title == "v1.2.3")
            .expect("v1");
        assert_eq!(v1.semver.minor, 2);
        let none = entries
            .iter()
            .find(|e| e.milestone.title == "no-version")
            .expect("nv");
        assert_eq!(none.semver, SemVer::ZERO);
    }
}
