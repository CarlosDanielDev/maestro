use super::client::GitHubClient;
use super::types::MaestroLabel;
use anyhow::Result;

/// Manages the maestro label lifecycle on GitHub issues.
pub struct LabelManager<C: GitHubClient> {
    client: C,
}

impl<C: GitHubClient> LabelManager<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    /// Transition: maestro:ready -> maestro:in-progress
    /// Adds new label first to avoid orphaned state on partial failure.
    pub async fn mark_in_progress(&self, issue_number: u64) -> Result<()> {
        self.client
            .add_label(issue_number, MaestroLabel::InProgress.as_str())
            .await?;
        self.client
            .remove_label(issue_number, MaestroLabel::Ready.as_str())
            .await
    }

    /// Transition: maestro:in-progress -> maestro:done
    /// Adds new label first to avoid orphaned state on partial failure.
    pub async fn mark_done(&self, issue_number: u64) -> Result<()> {
        self.client
            .add_label(issue_number, MaestroLabel::Done.as_str())
            .await?;
        self.client
            .remove_label(issue_number, MaestroLabel::InProgress.as_str())
            .await
    }

    /// Transition: maestro:in-progress -> maestro:failed
    /// Adds new label first to avoid orphaned state on partial failure.
    pub async fn mark_failed(&self, issue_number: u64) -> Result<()> {
        self.client
            .add_label(issue_number, MaestroLabel::Failed.as_str())
            .await?;
        self.client
            .remove_label(issue_number, MaestroLabel::InProgress.as_str())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::github::client::mock::MockGitHubClient;

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
}
