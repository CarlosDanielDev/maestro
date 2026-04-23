/// Issue type the user is creating. Drives conditional fields in the
/// `DorFields` step (Bug shows extra fields that Feature does not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IssueType {
    #[default]
    Feature,
    Bug,
}


/// Linear step machine for the Issue Wizard. Order is the displayed order
/// — `Self::ALL.iter().position(|s| s == self)` gives the step number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IssueWizardStep {
    /// Brief intro / context for the wizard.
    #[default]
    Context,
    /// Pick Feature vs Bug.
    TypeSelect,
    /// Title + Overview.
    BasicInfo,
    /// Multi-field DOR form.
    DorFields,
    /// Multi-select existing issues to set `Blocked By`.
    Dependencies,
    /// Optional AI review companion (#296).
    AiReview,
    /// Markdown preview before submit.
    Preview,
    /// Submitting to GitHub.
    Creating,
    /// Success.
    Complete,
    /// Submission failed.
    Failed,
}

impl IssueWizardStep {
    pub const ALL: &'static [Self] = &[
        Self::Context,
        Self::TypeSelect,
        Self::BasicInfo,
        Self::DorFields,
        Self::Dependencies,
        Self::AiReview,
        Self::Preview,
        Self::Creating,
        Self::Complete,
        Self::Failed,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Context => "Context",
            Self::TypeSelect => "Type Select",
            Self::BasicInfo => "Basic Info",
            Self::DorFields => "DOR Fields",
            Self::Dependencies => "Dependencies",
            Self::AiReview => "AI Review",
            Self::Preview => "Preview",
            Self::Creating => "Creating",
            Self::Complete => "Complete",
            Self::Failed => "Failed",
        }
    }

    /// 1-based step index for the progress indicator.
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

    /// The next step in the linear flow, or `None` if already at the end.
    pub fn next(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        Self::ALL.get(idx + 1).copied()
    }

    /// The previous step, or `None` if already at `Context`.
    pub fn previous(&self) -> Option<Self> {
        let idx = Self::ALL.iter().position(|s| s == self)?;
        if idx == 0 {
            None
        } else {
            Self::ALL.get(idx - 1).copied()
        }
    }

    pub fn is_first(&self) -> bool {
        matches!(self, Self::Context)
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
