use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MilestoneNaming {
    /// Infer naming from existing milestones (e.g. semver `vX.X.X`).
    Standard,
    /// Let Claude decide scope and naming (default: `MN: Description`).
    #[default]
    Ai,
    /// User-provided template pattern.
    Custom,
}

/// Configuration for the `maestro adapt` command.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptSettings {
    /// How milestones should be named in the generated plan.
    #[serde(default)]
    pub milestone_naming: MilestoneNaming,
    /// Custom template for milestone names (only used when `milestone_naming = "custom"`).
    /// Supports `{n}` (index) and `{title}` (description) placeholders.
    #[serde(default)]
    pub milestone_template: Option<String>,
}
