use super::transport::RepoProvider;
use crate::provider::types::MaestroLabel;
use anyhow::Result;
use std::future::Future;

/// How many times a single label call is attempted. One retry covers the
/// transient `gh` failures seen in the wild (TLS timeouts, #1140) without
/// masking persistent errors.
const LABEL_CALL_ATTEMPTS: u32 = 2;

/// Run one label API call, retrying once on failure before giving up.
async fn with_retry<F, Fut>(mut op: F) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut last_err = None;
    for attempt in 1..=LABEL_CALL_ATTEMPTS {
        match op().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!(
                    attempt,
                    attempts = LABEL_CALL_ATTEMPTS,
                    error = %e,
                    "label call failed"
                );
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("label call failed")))
}

/// Manages the maestro label lifecycle on GitHub issues.
pub struct LabelManager<C: RepoProvider> {
    client: C,
}

impl<C: RepoProvider> LabelManager<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    /// Transition: maestro:ready -> maestro:in-progress
    /// Adds new label first to avoid orphaned state on partial failure.
    pub async fn mark_in_progress(&self, issue_number: u64) -> Result<()> {
        with_retry(|| {
            self.client
                .add_label(issue_number, MaestroLabel::InProgress.as_str())
        })
        .await?;
        with_retry(|| {
            self.client
                .remove_label(issue_number, MaestroLabel::Ready.as_str())
        })
        .await
    }

    /// Transition: maestro:in-progress (or maestro:ready) -> maestro:done
    /// Adds new label first to avoid orphaned state on partial failure.
    /// Also clears `maestro:ready`: sessions started before #1140 — or whose
    /// start-time transition failed — otherwise keep `ready` next to `done`.
    pub async fn mark_done(&self, issue_number: u64) -> Result<()> {
        with_retry(|| {
            self.client
                .add_label(issue_number, MaestroLabel::Done.as_str())
        })
        .await?;
        with_retry(|| {
            self.client
                .remove_label(issue_number, MaestroLabel::InProgress.as_str())
        })
        .await?;
        with_retry(|| {
            self.client
                .remove_label(issue_number, MaestroLabel::Ready.as_str())
        })
        .await
    }

    /// Transition: maestro:in-progress -> maestro:failed
    /// Adds new label first to avoid orphaned state on partial failure.
    pub async fn mark_failed(&self, issue_number: u64) -> Result<()> {
        with_retry(|| {
            self.client
                .add_label(issue_number, MaestroLabel::Failed.as_str())
        })
        .await?;
        with_retry(|| {
            self.client
                .remove_label(issue_number, MaestroLabel::InProgress.as_str())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::github::transport::mock::MockGitHubClient;

    #[tokio::test]
    async fn mark_in_progress_removes_ready_adds_in_progress() {
        let client = MockGitHubClient::new();
        let mgr = LabelManager::new(client.clone());

        mgr.mark_in_progress(10).await.unwrap();

        let removes = client.remove_label_calls();
        let adds = client.add_label_calls();

        assert!(
            removes
                .iter()
                .any(|(n, l)| *n == 10 && l == "maestro:ready"),
            "expected maestro:ready to be removed"
        );
        assert!(
            adds.iter()
                .any(|(n, l)| *n == 10 && l == "maestro:in-progress"),
            "expected maestro:in-progress to be added"
        );
    }

    #[tokio::test]
    async fn mark_in_progress_propagates_remove_error() {
        let client = MockGitHubClient::new();
        client.set_remove_label_error("network failure");
        let mgr = LabelManager::new(client);
        let result = mgr.mark_in_progress(10).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mark_in_progress_propagates_add_error() {
        let client = MockGitHubClient::new();
        client.set_add_label_error("label missing");
        let mgr = LabelManager::new(client);
        let result = mgr.mark_in_progress(10).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mark_done_removes_in_progress_adds_done() {
        let client = MockGitHubClient::new();
        let mgr = LabelManager::new(client.clone());

        mgr.mark_done(7).await.unwrap();

        let removes = client.remove_label_calls();
        let adds = client.add_label_calls();

        assert!(
            removes
                .iter()
                .any(|(n, l)| *n == 7 && l == "maestro:in-progress"),
            "expected maestro:in-progress to be removed"
        );
        assert!(
            adds.iter().any(|(n, l)| *n == 7 && l == "maestro:done"),
            "expected maestro:done to be added"
        );
    }

    #[tokio::test]
    async fn mark_done_propagates_error() {
        let client = MockGitHubClient::new();
        client.set_add_label_error("forbidden");
        let mgr = LabelManager::new(client);
        assert!(mgr.mark_done(1).await.is_err());
    }

    #[tokio::test]
    async fn mark_failed_removes_in_progress_adds_failed() {
        let client = MockGitHubClient::new();
        let mgr = LabelManager::new(client.clone());

        mgr.mark_failed(3).await.unwrap();

        let removes = client.remove_label_calls();
        let adds = client.add_label_calls();

        assert!(
            removes
                .iter()
                .any(|(n, l)| *n == 3 && l == "maestro:in-progress"),
            "expected maestro:in-progress to be removed"
        );
        assert!(
            adds.iter().any(|(n, l)| *n == 3 && l == "maestro:failed"),
            "expected maestro:failed to be added"
        );
    }

    #[tokio::test]
    async fn mark_failed_propagates_error() {
        let client = MockGitHubClient::new();
        client.set_remove_label_error("server error");
        let mgr = LabelManager::new(client);
        assert!(mgr.mark_failed(1).await.is_err());
    }

    // #1140: mark_done must also clear maestro:ready — issues run via
    // `run --issue` (or whose start-time transition failed) otherwise keep
    // `maestro:ready` alongside `maestro:done`.

    #[tokio::test]
    async fn mark_done_also_removes_ready() {
        let client = MockGitHubClient::new();
        let mgr = LabelManager::new(client.clone());

        mgr.mark_done(9).await.unwrap();

        assert!(
            client
                .remove_label_calls()
                .iter()
                .any(|(n, l)| *n == 9 && l == "maestro:ready"),
            "expected maestro:ready to be removed"
        );
    }

    #[tokio::test]
    async fn mark_done_adds_done_before_removing_old_labels() {
        let client = MockGitHubClient::new();
        let mgr = LabelManager::new(client.clone());

        mgr.mark_done(5).await.unwrap();

        assert_eq!(client.add_label_calls()[0].1, "maestro:done");
        let removes = client.remove_label_calls();
        assert_eq!(removes[0].1, "maestro:in-progress");
        assert_eq!(removes[1].1, "maestro:ready");
    }

    // #1140: a transient label failure (e.g. TLS timeout) is retried once
    // before the caller logs it as failed.

    #[tokio::test]
    async fn mark_done_retries_add_after_transient_failure() {
        let client = MockGitHubClient::new();
        client.set_add_label_error_once("TLS handshake timeout");
        let mgr = LabelManager::new(client.clone());

        mgr.mark_done(4).await.expect("retry should succeed");

        assert_eq!(client.add_label_attempts(), 2);
        assert_eq!(client.add_label_calls().len(), 1);
    }

    #[tokio::test]
    async fn mark_in_progress_retries_remove_after_transient_failure() {
        let client = MockGitHubClient::new();
        client.set_remove_label_error_once("TLS handshake timeout");
        let mgr = LabelManager::new(client.clone());

        mgr.mark_in_progress(4).await.expect("retry should succeed");

        assert_eq!(client.remove_label_attempts(), 2);
        assert_eq!(client.remove_label_calls().len(), 1);
    }

    #[tokio::test]
    async fn mark_done_fails_after_retry_exhausted() {
        let client = MockGitHubClient::new();
        client.set_add_label_error("label missing");
        let mgr = LabelManager::new(client.clone());

        assert!(mgr.mark_done(4).await.is_err());
        assert_eq!(client.add_label_attempts(), 2);
    }
}
