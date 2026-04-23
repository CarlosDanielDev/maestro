/// Linear step machine for the AI-guided Milestone Wizard. Order is the
/// displayed order — `Self::ALL.iter().position(|s| s == self)` gives the
/// step number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MilestoneWizardStep {
    #[default]
    GoalDefinition,
    NonGoals,
    DocReferences,
    AiStructuring,
    ReviewPlan,
    Preview,
    Materializing,
    Complete,
    Failed,
}

impl MilestoneWizardStep {
    pub const ALL: &'static [Self] = &[
        Self::GoalDefinition,
        Self::NonGoals,
        Self::DocReferences,
        Self::AiStructuring,
        Self::ReviewPlan,
        Self::Preview,
        Self::Materializing,
        Self::Complete,
        Self::Failed,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::GoalDefinition => "Goal Definition",
            Self::NonGoals => "Non-Goals",
            Self::DocReferences => "Doc References",
            Self::AiStructuring => "AI Structuring",
            Self::ReviewPlan => "Review Plan",
            Self::Preview => "Preview",
            Self::Materializing => "Materializing",
            Self::Complete => "Complete",
            Self::Failed => "Failed",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL
            .iter()
            .position(|s| s == self)
            .map(|i| i + 1)
            .unwrap_or(1)
    }

    pub const fn total() -> usize {
        Self::ALL.len()
    }

    pub fn next(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        Self::ALL.get(idx + 1).copied()
    }

    pub fn previous(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        if idx == 0 {
            None
        } else {
            Self::ALL.get(idx - 1).copied()
        }
    }

    pub fn is_first(&self) -> bool {
        matches!(self, Self::GoalDefinition)
    }
}

/// Inputs the user has supplied so far. Sent to `claude --print` during
/// the `AiStructuring` step to produce an `AiGeneratedPlan`.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Reason: fields read by AiStructuring step (#294 + #297)
pub struct MilestonePlanPayload {
    pub goals: String,
    pub non_goals: String,
    /// Each entry is a file path or URL the user wants the AI to consider.
    pub doc_references: Vec<String>,
    /// Path validation result for each entry — `true` if the path exists or
    /// the entry is a URL. Index-aligned with `doc_references`.
    pub doc_reference_valid: Vec<bool>,
}

/// One AI-proposed issue that will be materialized into GitHub. The
/// `accepted` flag is toggled by the user during the `ReviewPlan` step.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Reason: shape consumed by Review/Preview/Materializing in #297
pub struct AiProposedIssue {
    pub title: String,
    pub overview: String,
    /// Indices into the surrounding `Vec<AiProposedIssue>` that this issue
    /// depends on (its `Blocked By`).
    pub blocked_by: Vec<usize>,
    pub accepted: bool,
}

/// Result of the `AiStructuring` step: a milestone title + description, plus
/// a list of proposed issues with dependency edges.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Reason: consumed by Review/Preview/Materializing in #297
pub struct AiGeneratedPlan {
    pub milestone_title: String,
    pub milestone_description: String,
    pub issues: Vec<AiProposedIssue>,
}
