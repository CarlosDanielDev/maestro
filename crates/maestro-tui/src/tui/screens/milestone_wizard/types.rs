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

    /// The stepper phase this step belongs to (§02.3: four real steps).
    /// Non-goals is part of Goals; AI structuring is Plan's loading state;
    /// creating, done and failed are states of Create.
    pub const fn phase(&self) -> WizardPhase {
        match self {
            Self::GoalDefinition | Self::NonGoals => WizardPhase::Goals,
            Self::DocReferences => WizardPhase::References,
            Self::AiStructuring | Self::ReviewPlan => WizardPhase::Plan,
            Self::Preview | Self::Materializing | Self::Complete | Self::Failed => {
                WizardPhase::Create
            }
        }
    }
}

/// The four phases the stepper shows (`MilestoneWizardStep::phase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardPhase {
    Goals,
    References,
    Plan,
    Create,
}

impl WizardPhase {
    pub const LABELS: [&'static str; 4] = ["Goals", "References", "Plan", "Create"];

    /// 0-based position in [`Self::LABELS`].
    pub const fn position(self) -> usize {
        self as usize
    }
}

/// Selected issues that `level_buckets()` would never place: each entry is
/// `(issue, blockers)` where every blocker is deselected or out of range.
/// Advancing past the Plan step is blocked while this is non-empty.
pub fn stranded(issues: &[AiProposedIssue]) -> Vec<(usize, Vec<usize>)> {
    issues
        .iter()
        .enumerate()
        .filter(|(_, it)| it.accepted)
        .filter_map(|(i, it)| {
            let missing: Vec<usize> = it
                .blocked_by
                .iter()
                .copied()
                .filter(|&d| issues.get(d).is_none_or(|dep| !dep.accepted))
                .collect();
            (!missing.is_empty()).then_some((i, missing))
        })
        .collect()
}

/// Inputs the user has supplied so far. Sent to `claude --print` during
/// the `AiStructuring` step to produce an `AiGeneratedPlan`.
#[derive(Debug, Clone, Default)]
pub struct MilestonePlanPayload {
    pub goals: String,
    pub non_goals: String,
    /// Each entry is a file path or URL the user wants the AI to consider.
    pub doc_references: Vec<String>,
    /// Path validation result for each entry — `true` if the path exists or
    /// the entry is a URL. Index-aligned with `doc_references`.
    pub doc_reference_valid: Vec<bool>,
    /// Image attachments collected via paste. Sent to the AI planner as
    /// additional references alongside the doc paths.
    pub image_paths: Vec<String>,
}

/// One AI-proposed issue that will be materialized into GitHub. The
/// `accepted` flag is toggled by the user during the `ReviewPlan` step.
#[derive(Debug, Clone)]
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
pub struct AiGeneratedPlan {
    pub milestone_title: String,
    pub milestone_description: String,
    pub issues: Vec<AiProposedIssue>,
    /// What an earlier, partial create already wrote (#1284). A retry
    /// reuses these by number, never by title, and creates only the rest.
    pub written: Option<PlanWritten>,
}

/// The milestone and the issues (plan index → number) a partial create
/// wrote. Kept on the plan so the retry's `CreateMilestoneWithIssues`
/// carries it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanWritten {
    pub milestone_number: u64,
    pub issues: std::collections::BTreeMap<usize, u64>,
}
