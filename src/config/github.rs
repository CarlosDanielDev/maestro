use crate::provider::types::ProviderKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    Merge,
    #[default]
    Squash,
    Rebase,
}

impl MergeMethod {
    pub fn flag(&self) -> &'static str {
        match self {
            Self::Merge => "--merge",
            Self::Squash => "--squash",
            Self::Rebase => "--rebase",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GithubConfig {
    #[serde(default = "default_issue_labels")]
    pub issue_filter_labels: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_pr: bool,
    /// Cache TTL for issue data in seconds. Default: 300 (5 min).
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
    /// Whether to auto-merge PRs after all gates pass. Default: false.
    #[serde(default)]
    pub auto_merge: bool,
    /// Merge method. Default: Squash.
    #[serde(default)]
    pub merge_method: MergeMethod,
}

impl Default for GithubConfig {
    fn default() -> Self {
        Self {
            issue_filter_labels: default_issue_labels(),
            auto_pr: true,
            cache_ttl_secs: default_cache_ttl(),
            auto_merge: false,
            merge_method: MergeMethod::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider type: "github" or "azure_devops". Default: github.
    #[serde(default)]
    pub kind: ProviderKind,
    /// Issue/work-item filter labels/tags.
    #[serde(default = "default_issue_labels")]
    pub issue_filter_labels: Vec<String>,
    /// Whether to auto-create PRs on session completion.
    #[serde(default = "default_true")]
    pub auto_pr: bool,
    /// Whether to auto-merge PRs after gates pass.
    #[serde(default)]
    pub auto_merge: bool,
    /// Merge method.
    #[serde(default)]
    pub merge_method: MergeMethod,
    /// Cache TTL for issue data in seconds.
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
    /// Azure DevOps organization URL (e.g., "https://dev.azure.com/MyOrg").
    #[serde(default)]
    pub organization: Option<String>,
    /// Azure DevOps project name.
    #[serde(default)]
    pub az_project: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            kind: ProviderKind::default(),
            issue_filter_labels: default_issue_labels(),
            auto_pr: true,
            auto_merge: false,
            merge_method: MergeMethod::default(),
            cache_ttl_secs: default_cache_ttl(),
            organization: None,
            az_project: None,
        }
    }
}

fn default_issue_labels() -> Vec<String> {
    vec!["maestro:ready".into()]
}
fn default_true() -> bool {
    true
}
fn default_cache_ttl() -> u64 {
    300
}
