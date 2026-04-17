#![allow(dead_code)] // Reason: multi-provider support (Azure DevOps) — planned feature
pub mod azure_devops;
pub mod github;
pub mod types;

use crate::config::ProviderConfig;
use self::github::client::GitHubClient;
use anyhow::Result;
use types::ProviderKind;

// Re-export GitHub client for use by the factory.
use self::github::client::GhCliClient;

/// Create the appropriate provider client from config.
pub fn create_provider(config: &ProviderConfig) -> Result<Box<dyn GitHubClient>> {
    match config.kind {
        ProviderKind::Github => Ok(Box::new(GhCliClient::new())),
        ProviderKind::AzureDevops => {
            let org = config.organization.clone().ok_or_else(|| {
                anyhow::anyhow!("provider.organization required for azure_devops")
            })?;
            let project = config
                .az_project
                .clone()
                .ok_or_else(|| anyhow::anyhow!("provider.az_project required for azure_devops"))?;
            Ok(Box::new(azure_devops::AzDevOpsClient::new(org, project)))
        }
    }
}

/// Detect provider from a git remote URL string.
pub fn detect_provider_from_remote(url: &str) -> ProviderKind {
    if url.contains("dev.azure.com") || url.contains("visualstudio.com") {
        ProviderKind::AzureDevops
    } else {
        ProviderKind::Github
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // detect_provider_from_remote

    #[test]
    fn detect_github_ssh_remote() {
        assert_eq!(
            detect_provider_from_remote("git@github.com:owner/repo.git"),
            ProviderKind::Github
        );
    }

    #[test]
    fn detect_github_https_remote() {
        assert_eq!(
            detect_provider_from_remote("https://github.com/owner/repo.git"),
            ProviderKind::Github
        );
    }

    #[test]
    fn detect_azure_devops_ssh_remote() {
        assert_eq!(
            detect_provider_from_remote("git@ssh.dev.azure.com:v3/MyOrg/MyProject/MyRepo"),
            ProviderKind::AzureDevops
        );
    }

    #[test]
    fn detect_azure_devops_https_remote() {
        assert_eq!(
            detect_provider_from_remote("https://MyOrg@dev.azure.com/MyOrg/MyProject/_git/MyRepo"),
            ProviderKind::AzureDevops
        );
    }

    #[test]
    fn detect_azure_devops_visualstudio_legacy() {
        assert_eq!(
            detect_provider_from_remote("https://MyOrg.visualstudio.com/MyProject/_git/MyRepo"),
            ProviderKind::AzureDevops
        );
    }

    #[test]
    fn detect_unknown_remote_defaults_to_github() {
        assert_eq!(
            detect_provider_from_remote("https://bitbucket.org/owner/repo.git"),
            ProviderKind::Github
        );
    }

    #[test]
    fn detect_empty_remote_defaults_to_github() {
        assert_eq!(detect_provider_from_remote(""), ProviderKind::Github);
    }

    // create_provider

    #[test]
    fn create_provider_github() {
        let cfg = ProviderConfig::default();
        let _client = create_provider(&cfg).unwrap();
    }

    #[test]
    fn create_provider_azure_devops_missing_org_returns_err() {
        let cfg = ProviderConfig {
            kind: ProviderKind::AzureDevops,
            organization: None,
            az_project: Some("MyProject".into()),
            ..ProviderConfig::default()
        };
        assert!(create_provider(&cfg).is_err());
    }

    #[test]
    fn create_provider_azure_devops_missing_project_returns_err() {
        let cfg = ProviderConfig {
            kind: ProviderKind::AzureDevops,
            organization: Some("MyOrg".into()),
            az_project: None,
            ..ProviderConfig::default()
        };
        assert!(create_provider(&cfg).is_err());
    }

    #[test]
    fn create_provider_azure_devops_with_both_fields() {
        let cfg = ProviderConfig {
            kind: ProviderKind::AzureDevops,
            organization: Some("https://dev.azure.com/MyOrg".into()),
            az_project: Some("MyProject".into()),
            ..ProviderConfig::default()
        };
        let _client = create_provider(&cfg).unwrap();
    }
}
