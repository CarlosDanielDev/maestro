/// Issue type the user is creating, set on the Basics Type row. Drives
/// conditional fields in the `DorFields` step (Bug shows extra fields that
/// Feature does not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IssueType {
    #[default]
    Feature,
    Bug,
}

/// Step machine for the Issue Wizard (§02.2). The five real steps are
/// [`Self::STEPPER`]; `Creating`, `Complete` and `Failed` are states of the
/// create gate on Preview, not steps of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IssueWizardStep {
    /// Title, the Type row (Feature or Bug) and Overview.
    #[default]
    BasicInfo,
    /// Multi-field DOR form.
    DorFields,
    /// Multi-select existing issues to set `Blocked By`.
    Dependencies,
    /// Optional AI review companion (#296).
    AiReview,
    /// Markdown preview and the create gate.
    Preview,
    /// Submitting to GitHub.
    Creating,
    /// Success.
    Complete,
    /// Submission failed.
    Failed,
}

impl IssueWizardStep {
    /// The steps the stepper shows, in order.
    pub const STEPPER: [Self; 5] = [
        Self::BasicInfo,
        Self::DorFields,
        Self::Dependencies,
        Self::AiReview,
        Self::Preview,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::BasicInfo => "Basics",
            Self::DorFields => "Details",
            Self::Dependencies => "Blocked by",
            Self::AiReview => "AI review",
            Self::Preview => "Preview",
            Self::Creating => "Creating",
            Self::Complete => "Complete",
            Self::Failed => "Failed",
        }
    }

    /// 0-based position on the stepper. The create states sit on Preview.
    pub fn stepper_index(&self) -> usize {
        Self::STEPPER
            .iter()
            .position(|s| s == self)
            .unwrap_or(Self::STEPPER.len() - 1)
    }

    /// The next step in the linear flow, or `None` on Preview and after.
    pub fn next(&self) -> Option<Self> {
        let idx = Self::STEPPER.iter().position(|s| s == self)?;
        Self::STEPPER.get(idx + 1).copied()
    }

    /// The previous step, or `None` on Basics.
    pub fn previous(&self) -> Option<Self> {
        let idx = Self::STEPPER.iter().position(|s| s == self)?;
        idx.checked_sub(1)
            .and_then(|i| Self::STEPPER.get(i).copied())
    }

    pub fn is_first(&self) -> bool {
        matches!(self, Self::BasicInfo)
    }
}

/// Everything the user has entered (or the AI has helped them refine) so
/// far. This payload travels with the wizard and is shipped to GitHub on
/// the `Creating` step.
#[derive(Debug, Clone, Default)]
pub struct IssueCreationPayload {
    pub issue_type: IssueType,
    pub title: String,
    pub overview: String,
    /// Required for both feature and bug.
    pub expected_behavior: String,
    /// Bug-only.
    pub current_behavior: String,
    /// Bug-only.
    pub steps_to_reproduce: String,
    pub acceptance_criteria: String,
    pub files_to_modify: String,
    pub test_hints: String,
    /// Issue numbers selected in the Dependencies step. Empty → "None".
    pub blocked_by: Vec<u64>,
    /// Optional milestone number to attach the new issue to.
    pub milestone: Option<u64>,
    /// Image attachments collected via paste (Cmd+V with an image on the
    /// clipboard) or bracketed paste of a file path. Rendered into the
    /// body as `[Attached image: <path>]` references.
    pub image_paths: Vec<String>,
}

impl IssueCreationPayload {
    pub fn new() -> Self {
        Self::default()
    }
}
