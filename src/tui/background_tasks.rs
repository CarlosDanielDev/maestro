use super::app;
use super::screens;
use crate::provider::github::client::{GhCliClient, GitHubClient};

pub(super) fn spawn_issue_fetch(
    tx: tokio::sync::mpsc::UnboundedSender<app::TuiDataEvent>,
    config: screens::SessionConfig,
) {
    let custom_prompt = config.custom_prompt.clone();
    match config.issue_number {
        Some(issue_number) => {
            tokio::spawn(async move {
                let client = GhCliClient::new();
                let result = client.get_issue(issue_number).await;
                let _ = tx.send(app::TuiDataEvent::Issue(result, custom_prompt));
            });
        }
        None => {
            let _ = tx.send(app::TuiDataEvent::Issue(
                Err(anyhow::anyhow!(
                    "Cannot launch session without an issue number"
                )),
                custom_prompt,
            ));
        }
    }
}

/// Spawn a non-blocking version check that sends the result via the data channel.
pub(crate) fn spawn_version_check(tx: tokio::sync::mpsc::UnboundedSender<app::TuiDataEvent>) {
    tokio::spawn(async move {
        use crate::updater::checker::{GitHubReleaseChecker, UpdateChecker};
        let checker = GitHubReleaseChecker::new(crate::updater::GITHUB_REPO.to_string());
        match checker.check_for_update().await {
            Ok(info) => {
                let _ = tx.send(app::TuiDataEvent::VersionCheckResult(info));
            }
            Err(e) => {
                tracing::debug!("Version check failed: {}", e);
            }
        }
    });
}

/// Spawn background binary download and installation.
pub(super) fn spawn_upgrade_download(
    tx: tokio::sync::mpsc::UnboundedSender<app::TuiDataEvent>,
    info: crate::updater::ReleaseInfo,
) {
    let dest = std::env::current_exe().unwrap_or_default();
    tokio::spawn(async move {
        let installer = crate::updater::installer::Installer::new(dest);
        match installer.download_and_install(&info.download_url).await {
            Ok(backup) => {
                let _ = tx.send(app::TuiDataEvent::UpgradeResult(Ok(backup)));
            }
            Err(e) => {
                let _ = tx.send(app::TuiDataEvent::UpgradeResult(Err(e.to_string())));
            }
        }
    });
}
