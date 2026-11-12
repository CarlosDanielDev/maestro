//! Blocked-by dependency gate for `maestro run --issue` (#1141).
//!
//! `maestro queue` shows an issue as Blocked when its `blocked-by` list has
//! open entries, but `run --issue` used to spawn a session regardless. This
//! gate refuses the spawn while any blocker is still open, naming the
//! blockers and pointing at `--ignore-deps` as the explicit override.

use anyhow::Context;

use crate::provider::github::client::RepoProvider;
use crate::provider::types::Issue;

/// States that mean a blocker no longer blocks. GitHub parsing lowercases
/// `state` to "open"/"closed"; the Azure DevOps provider passes its process
/// states through verbatim ("Closed", "Done", "Active", ...), so match
/// case-insensitively and treat every non-terminal state as still blocking.
fn is_settled(state: &str) -> bool {
    matches!(
        state.to_ascii_lowercase().as_str(),
        "closed" | "done" | "completed"
    )
}

/// Refuse to run `issue` while any of its `blocked-by` dependencies is open.
/// Errors name the open blockers and point at `--ignore-deps` as the way to
/// run anyway.
pub(crate) async fn ensure_issue_unblocked(
    client: &dyn RepoProvider,
    issue: &Issue,
) -> anyhow::Result<()> {
    let blockers = issue.all_blockers();
    if blockers.is_empty() {
        return Ok(());
    }

    let mut open = Vec::new();
    for blocker in blockers {
        let blocker_issue = client.get_issue(blocker).await.with_context(|| {
            format!(
                "Failed to check blocker #{} of #{}; pass --ignore-deps to run anyway",
                blocker, issue.number
            )
        })?;
        if !is_settled(&blocker_issue.state) {
            open.push(blocker);
        }
    }

    if open.is_empty() {
        return Ok(());
    }

    anyhow::bail!(
        "#{} is blocked by {} (open); pass --ignore-deps to run anyway",
        issue.number,
        open.iter()
            .map(|n| format!("#{n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::github::transport::mock::MockGitHubClient;

    fn issue(number: u64, body: &str, labels: Vec<String>, state: &str) -> Issue {
        Issue {
            number,
            title: format!("Issue {}", number),
            body: body.to_string(),
            labels,
            state: state.to_string(),
            html_url: String::new(),
            milestone: None,
            assignees: vec![],
        }
    }

    #[tokio::test]
    async fn refuses_when_body_blocker_is_open() {
        let client = MockGitHubClient::new();
        client.set_issues(vec![
            issue(1, "", vec![], "open"),
            issue(3, "blocked-by: #1", vec![], "open"),
        ]);
        let target = issue(3, "blocked-by: #1", vec![], "open");

        let err = ensure_issue_unblocked(&client, &target)
            .await
            .expect_err("open blocker must refuse the run");
        let msg = err.to_string();
        assert!(
            msg.contains("#3 is blocked by #1 (open); pass --ignore-deps to run anyway"),
            "message was: {msg}"
        );
    }

    #[tokio::test]
    async fn refuses_when_label_blocker_is_open() {
        let client = MockGitHubClient::new();
        client.set_issues(vec![
            issue(1, "", vec![], "open"),
            issue(3, "", vec!["blocked-by:#1".to_string()], "open"),
        ]);
        let target = issue(3, "", vec!["blocked-by:#1".to_string()], "open");

        let err = ensure_issue_unblocked(&client, &target)
            .await
            .expect_err("open label blocker must refuse the run");
        assert!(
            err.to_string().contains("#3 is blocked by #1 (open)"),
            "message was: {err}"
        );
    }

    #[tokio::test]
    async fn lists_every_open_blocker() {
        let client = MockGitHubClient::new();
        client.set_issues(vec![
            issue(1, "", vec![], "open"),
            issue(4, "", vec![], "open"),
            issue(3, "blocked-by: #1\nblocked-by: #4", vec![], "open"),
        ]);
        let target = issue(3, "blocked-by: #1\nblocked-by: #4", vec![], "open");

        let err = ensure_issue_unblocked(&client, &target)
            .await
            .expect_err("open blockers must refuse the run");
        let msg = err.to_string();
        assert!(
            msg.contains("#3 is blocked by #1, #4 (open)"),
            "message was: {msg}"
        );
    }

    #[tokio::test]
    async fn runs_when_all_blockers_are_closed() {
        let client = MockGitHubClient::new();
        client.set_issues(vec![
            issue(1, "", vec![], "closed"),
            issue(3, "blocked-by: #1", vec![], "open"),
        ]);
        let target = issue(3, "blocked-by: #1", vec![], "open");

        ensure_issue_unblocked(&client, &target)
            .await
            .expect("closed blocker must not refuse the run");
    }

    #[tokio::test]
    async fn treats_done_and_completed_states_as_settled() {
        let client = MockGitHubClient::new();
        client.set_issues(vec![
            issue(1, "", vec![], "Done"),
            issue(2, "", vec![], "COMPLETED"),
            issue(3, "blocked-by: #1\nblocked-by: #2", vec![], "open"),
        ]);
        let target = issue(3, "blocked-by: #1\nblocked-by: #2", vec![], "open");

        ensure_issue_unblocked(&client, &target)
            .await
            .expect("non-GitHub terminal states must not refuse the run");
    }

    #[tokio::test]
    async fn runs_when_there_are_no_blockers() {
        let client = MockGitHubClient::new();
        let target = issue(3, "no dependencies here", vec![], "open");

        ensure_issue_unblocked(&client, &target)
            .await
            .expect("issue without blockers must run as today");
    }

    #[tokio::test]
    async fn refuses_when_blocker_cannot_be_fetched() {
        let client = MockGitHubClient::new();
        client.set_issues(vec![issue(3, "blocked-by: #1", vec![], "open")]);
        client.set_get_issue_error(1, "gh: transient API error");
        let target = issue(3, "blocked-by: #1", vec![], "open");

        let err = ensure_issue_unblocked(&client, &target)
            .await
            .expect_err("unverifiable blocker must refuse the run");
        let msg = err.to_string();
        assert!(msg.contains("#1"), "message was: {msg}");
        assert!(msg.contains("--ignore-deps"), "message was: {msg}");
    }
}
