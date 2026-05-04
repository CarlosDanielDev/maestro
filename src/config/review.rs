use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewConfig {
    /// Whether review dispatch is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Review command template (used when `reviewers` is empty). Variables: {pr_number}, {branch}.
    #[serde(default = "default_review_command")]
    pub command: String,
    /// Multi-reviewer council configuration. If non-empty, overrides `command`.
    #[serde(default)]
    pub reviewers: Vec<ReviewerEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewerEntry {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub required: bool,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: default_review_command(),
            reviewers: Vec::new(),
        }
    }
}

fn default_review_command() -> String {
    "gh pr review {pr_number} --comment --body 'Automated review by Maestro'".into()
}
