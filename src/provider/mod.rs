pub mod azure_devops;
pub mod github;
pub mod terms;
pub mod types;

pub use self::github::client::{CreateOutcome, RepoProvider};
use crate::config::ProviderConfig;
use anyhow::Result;
use types::ProviderKind;

// Re-export GitHub client for use by the factory.
use self::github::client::GhCliClient;

/// Create the appropriate provider client from config.
pub fn create_provider(config: &ProviderConfig) -> Result<Box<dyn RepoProvider>> {
    match config.kind {
        ProviderKind::Github => Ok(Box::new(GhCliClient::from_config_repo(config.repo.clone()))),
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

/// Extract the `owner/repo` slug from a GitHub remote URL. Accepts
/// `https://github.com/owner/repo(.git)` and the scp-like
/// `git@github.com:owner/repo(.git)`; returns `None` for other hosts or
/// shapes so callers fall back to an empty `repo` key.
pub fn github_owner_repo(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let idx = trimmed.find("github.com")?;
    let rest = &trimmed[idx + "github.com".len()..];
    let rest = rest
        .strip_prefix(':')
        .or_else(|| rest.strip_prefix('/'))
        .unwrap_or(rest);
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    Some(format!(
        "{}/{}",
        segments[segments.len() - 2],
        segments[segments.len() - 1]
    ))
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

    // github_owner_repo

    #[test]
    fn github_owner_repo_parses_https_url_with_git_suffix() {
        assert_eq!(
            github_owner_repo("https://github.com/acme/widgets.git").as_deref(),
            Some("acme/widgets")
        );
    }

    #[test]
    fn github_owner_repo_parses_https_url_without_git_suffix() {
        assert_eq!(
            github_owner_repo("https://github.com/acme/widgets").as_deref(),
            Some("acme/widgets")
        );
    }

    #[test]
    fn github_owner_repo_parses_scp_like_ssh_url() {
        assert_eq!(
            github_owner_repo("git@github.com:acme/widgets.git").as_deref(),
            Some("acme/widgets")
        );
    }

    #[test]
    fn github_owner_repo_parses_ssh_scheme_with_userinfo() {
        assert_eq!(
            github_owner_repo("ssh://git@github.com/acme/widgets.git").as_deref(),
            Some("acme/widgets")
        );
    }

    #[test]
    fn github_owner_repo_rejects_non_github_hosts() {
        assert_eq!(
            github_owner_repo("https://bitbucket.org/acme/widgets.git"),
            None
        );
        assert_eq!(
            github_owner_repo("https://dev.azure.com/org/proj/_git/repo"),
            None
        );
        assert_eq!(github_owner_repo(""), None);
    }

    #[test]
    fn github_owner_repo_rejects_single_path_segment() {
        assert_eq!(github_owner_repo("https://github.com/acme"), None);
    }

    // create_provider

    #[test]
    fn create_provider_github() {
        let cfg = ProviderConfig::default();
        let _client = create_provider(&cfg).unwrap();
    }

    #[test]
    fn gh_cli_client_is_repo_provider_trait_object() {
        let _: Box<dyn RepoProvider> = Box::new(GhCliClient::new());
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
