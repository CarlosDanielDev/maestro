pub mod ai_review;
mod draw;
pub mod types;

pub use ai_review::build_review_prompt;
pub use types::{IssueCreationPayload, IssueType, IssueWizardStep};

use super::prompt_input::{ClipboardContent, ClipboardProvider, SystemClipboard};
use super::wizard_fields::{TextAreaField, WizardFields};
use super::{Screen, ScreenAction};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders},
};

/// Identifier of a focusable text field inside a wizard step. Stable across
/// re-renders so the focused field survives a redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldId {
    Title,
    Overview,
    ExpectedBehavior,
    CurrentBehavior,
    StepsToReproduce,
    AcceptanceCriteria,
    FilesToModify,
    TestHints,
}

impl FieldId {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Overview => "Overview",
            Self::ExpectedBehavior => "Expected Behavior",
            Self::CurrentBehavior => "Current Behavior",
            Self::StepsToReproduce => "Steps to Reproduce",
            Self::AcceptanceCriteria => "Acceptance Criteria",
            Self::FilesToModify => "Files to Modify",
            Self::TestHints => "Test Hints",
        }
    }

    /// Whether this field accepts a newline on Shift+Enter.
    pub const fn is_multiline(&self) -> bool {
        !matches!(self, Self::Title)
    }

    fn payload_ref<'a>(&self, p: &'a IssueCreationPayload) -> &'a str {
        match self {
            Self::Title => &p.title,
            Self::Overview => &p.overview,
            Self::ExpectedBehavior => &p.expected_behavior,
            Self::CurrentBehavior => &p.current_behavior,
            Self::StepsToReproduce => &p.steps_to_reproduce,
            Self::AcceptanceCriteria => &p.acceptance_criteria,
            Self::FilesToModify => &p.files_to_modify,
            Self::TestHints => &p.test_hints,
        }
    }

    fn payload_ref_mut<'a>(&self, p: &'a mut IssueCreationPayload) -> &'a mut String {
        match self {
            Self::Title => &mut p.title,
            Self::Overview => &mut p.overview,
            Self::ExpectedBehavior => &mut p.expected_behavior,
            Self::CurrentBehavior => &mut p.current_behavior,
            Self::StepsToReproduce => &mut p.steps_to_reproduce,
            Self::AcceptanceCriteria => &mut p.acceptance_criteria,
            Self::FilesToModify => &mut p.files_to_modify,
            Self::TestHints => &mut p.test_hints,
        }
    }
}

pub struct IssueWizardScreen {
    step: IssueWizardStep,
    payload: IssueCreationPayload,
    /// Text fields for the current step. Rebuilt whenever the step
    /// changes so BasicInfo owns Title+Overview textareas, DorFields
    /// owns its 4 or 6 textareas, and non-text steps hold `empty()`.
    pub(super) fields: WizardFields,
    /// #295 Dependencies step state.
    pub(super) dep_issues: Option<Vec<crate::provider::github::types::GhIssue>>,
    pub(super) dep_loading: bool,
    pub(super) dep_selected: usize,
    pub(super) dep_checked: std::collections::BTreeSet<u64>,
    /// #296 AI Review step state.
    pub(super) review_loading: bool,
    pub(super) review_text: Option<String>,
    pub(super) review_error: Option<String>,
    /// Clipboard provider for Ctrl+V. Injected as a trait object so
    /// tests can supply a deterministic fake.
    clipboard: Box<dyn ClipboardProvider>,
    /// #298 Creating/Complete/Failed state.
    pub(super) create_in_flight: bool,
    /// True between `begin_create()` and the moment dispatch enqueues
    /// the `CreateIssue` command — guards against duplicate dispatches
    /// while still letting `create_in_flight` remain true until the
    /// background task completes.
    pub(super) create_enqueued: bool,
    pub(super) created_issue_number: Option<u64>,
    pub(super) create_error: Option<String>,
}

impl IssueWizardScreen {
    pub fn new() -> Self {
        Self::with_clipboard(Box::new(SystemClipboard))
    }

    pub fn with_clipboard(clipboard: Box<dyn ClipboardProvider>) -> Self {
        Self {
            step: IssueWizardStep::default(),
            payload: IssueCreationPayload::new(),
            fields: WizardFields::empty(),
            dep_issues: None,
            dep_loading: false,
            dep_selected: 0,
            dep_checked: std::collections::BTreeSet::new(),
            review_loading: false,
            review_text: None,
            review_error: None,
            clipboard,
            create_in_flight: false,
            create_enqueued: false,
            created_issue_number: None,
            create_error: None,
        }
    }

    /// Insert a pasted payload into the focused textarea. `TextAreaField`
    /// handles the control-char filter + single-line newline collapse.
    /// Syncs immediately so downstream readers of `payload` (preview
    /// markdown, AI prompts) see the pasted content without waiting for
    /// the next step transition.
    pub fn paste_text_into_focused(&mut self, text: &str) {
        if let Some(field) = self.fields.focused_mut() {
            field.insert_sanitized(text);
        }
        self.sync_fields_into_payload();
    }

    /// Read the system clipboard synchronously and route the content:
    /// image → `payload.image_paths`; text → focused field; anything
    /// else → no-op. Invoked on Ctrl+V.
    pub fn paste_from_clipboard(&mut self) {
        match self.clipboard.read() {
            ClipboardContent::Image(path) => {
                self.payload
                    .image_paths
                    .push(path.to_string_lossy().to_string());
            }
            ClipboardContent::Text(text) => {
                self.paste_text_into_focused(&text);
            }
            ClipboardContent::Empty | ClipboardContent::Unavailable => {}
        }
    }

    pub fn create_enqueued(&self) -> bool {
        self.create_enqueued
    }

    pub fn mark_create_enqueued(&mut self) {
        self.create_enqueued = true;
    }

    // ---- #298 Creation ----

    pub fn render_body_markdown(&self) -> String {
        render_body_markdown(&self.payload)
    }

    pub fn render_labels(&self) -> Vec<String> {
        render_labels(&self.payload)
    }

    pub fn create_in_flight(&self) -> bool {
        self.create_in_flight
    }

    pub fn created_issue_number(&self) -> Option<u64> {
        self.created_issue_number
    }

    pub fn create_error(&self) -> Option<&str> {
        self.create_error.as_deref()
    }

    pub fn begin_create(&mut self) {
        self.sync_fields_into_payload();
        self.create_in_flight = true;
        self.create_enqueued = false;
        self.create_error = None;
        self.created_issue_number = None;
        self.step = IssueWizardStep::Creating;
        self.rebuild_fields_for_step();
    }

    pub fn finish_create(&mut self, result: anyhow::Result<u64>) {
        self.create_in_flight = false;
        self.create_enqueued = false;
        match result {
            Ok(n) => {
                self.created_issue_number = Some(n);
                self.step = IssueWizardStep::Complete;
            }
            Err(e) => {
                self.create_error = Some(e.to_string());
                self.step = IssueWizardStep::Failed;
            }
        }
    }

    /// Reset to Context with a fresh payload — used by the Complete
    /// step's "Enter to create another" action.
    pub fn reset_for_another(&mut self) {
        *self = Self::new();
    }

    // ---- #296 AI Review ----

    pub fn begin_ai_review(&mut self) {
        self.sync_fields_into_payload();
        self.review_loading = true;
        self.review_text = None;
        self.review_error = None;
    }

    pub fn apply_ai_review(&mut self, result: Result<String, String>) {
        self.review_loading = false;
        match result {
            Ok(text) => {
                self.review_text = Some(text);
                self.review_error = None;
            }
            Err(e) => {
                self.review_error = Some(e);
                self.review_text = None;
            }
        }
    }

    pub fn review_loading(&self) -> bool {
        self.review_loading
    }

    pub fn review_text(&self) -> Option<&str> {
        self.review_text.as_deref()
    }

    pub fn review_error(&self) -> Option<&str> {
        self.review_error.as_deref()
    }

    pub fn entered_ai_review_step(&self) -> bool {
        matches!(self.step, IssueWizardStep::AiReview)
            && self.review_text.is_none()
            && !self.review_loading
            && self.review_error.is_none()
    }

    fn jump_to(&mut self, target: IssueWizardStep) {
        self.sync_fields_into_payload();
        self.step = target;
        self.rebuild_fields_for_step();
    }
}

/// Render a wizard payload to the GitHub-flavored markdown body. Used both
/// from the wizard screen (for the Preview step) and from the background
/// `CreateIssue` task in `tui::run`, which has no screen handle.
pub fn render_body_markdown(p: &IssueCreationPayload) -> String {
    let mut s = String::new();
    push_section(&mut s, "Overview", &p.overview);
    push_section(&mut s, "Expected Behavior", &p.expected_behavior);
    if matches!(p.issue_type, IssueType::Bug) {
        if !p.current_behavior.trim().is_empty() {
            push_section(&mut s, "Current Behavior", &p.current_behavior);
        }
        if !p.steps_to_reproduce.trim().is_empty() {
            push_section(&mut s, "Steps to Reproduce", &p.steps_to_reproduce);
        }
    }
    push_section(&mut s, "Acceptance Criteria", &p.acceptance_criteria);
    if !p.files_to_modify.trim().is_empty() {
        push_section(&mut s, "Files to Modify", &p.files_to_modify);
    }
    if !p.test_hints.trim().is_empty() {
        push_section(&mut s, "Test Hints", &p.test_hints);
    }
    let blocked = if p.blocked_by.is_empty() {
        "- None".to_string()
    } else {
        p.blocked_by
            .iter()
            .map(|n| format!("- #{}", n))
            .collect::<Vec<_>>()
            .join("\n")
    };
    push_section(&mut s, "Blocked By", &blocked);
    if !p.image_paths.is_empty() {
        let attachments = p
            .image_paths
            .iter()
            .map(|p| format!("- [Attached image: {}]", p))
            .collect::<Vec<_>>()
            .join("\n");
        push_section(&mut s, "Attachments", &attachments);
    }
    let dod = build_definition_of_done(&p.acceptance_criteria);
    push_section(&mut s, "Definition of Done", &dod);
    s
}

/// Labels applied to the new issue based on its type. Always includes
/// `maestro:ready` so the queue picks it up.
pub fn render_labels(p: &IssueCreationPayload) -> Vec<String> {
    vec![
        "maestro:ready".to_string(),
        match p.issue_type {
            IssueType::Feature => "enhancement".to_string(),
            IssueType::Bug => "bug".to_string(),
        },
    ]
}

fn push_section(out: &mut String, title: &str, body: &str) {
    out.push_str("## ");
    out.push_str(title);
    out.push_str("\n\n");
    out.push_str(body.trim());
    out.push_str("\n\n");
}

/// Convert `acceptance_criteria` into a checklist used as the
/// `Definition of Done`. Lines starting with `-` or `*` are normalised
/// into `- [ ] …`; other lines pass through.
fn build_definition_of_done(acceptance_criteria: &str) -> String {
    let trimmed = acceptance_criteria.trim();
    if trimmed.is_empty() {
        return "- [ ] All acceptance criteria met".to_string();
    }
    trimmed
        .lines()
        .map(|line| {
            let l = line.trim_start();
            if let Some(rest) = l.strip_prefix("- [ ]").or_else(|| l.strip_prefix("- [x]")) {
                format!("- [ ]{}", rest)
            } else if let Some(rest) = l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")) {
                format!("- [ ] {}", rest)
            } else {
                format!("- [ ] {}", l)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl IssueWizardScreen {
    /// #295: called by dispatch when entering the Dependencies step so the
    /// loading spinner shows immediately and the fetch is queued.
    pub fn begin_dependency_fetch(&mut self) {
        self.dep_issues = None;
        self.dep_loading = true;
        // Seed the checkbox set from any pre-existing payload (e.g. #326).
        self.dep_checked = self.payload.blocked_by.iter().copied().collect();
    }

    /// #295: apply the result of a `FetchIssues` background task.
    pub fn apply_dep_issues(&mut self, issues: Vec<crate::provider::github::types::GhIssue>) {
        // Filter to open issues only — closed issues can't block anything.
        let open: Vec<_> = issues.into_iter().filter(|i| i.state == "open").collect();
        self.dep_selected = self.dep_selected.min(open.len().saturating_sub(1));
        self.dep_issues = Some(open);
        self.dep_loading = false;
    }

    pub fn dep_issues(&self) -> Option<&[crate::provider::github::types::GhIssue]> {
        self.dep_issues.as_deref()
    }

    pub fn dep_loading(&self) -> bool {
        self.dep_loading
    }

    pub fn dep_selected(&self) -> usize {
        self.dep_selected
    }

    pub fn dep_is_checked(&self, issue_number: u64) -> bool {
        self.dep_checked.contains(&issue_number)
    }

    fn dep_toggle_selected(&mut self) {
        let Some(issues) = self.dep_issues.as_ref() else {
            return;
        };
        let Some(issue) = issues.get(self.dep_selected) else {
            return;
        };
        if !self.dep_checked.remove(&issue.number) {
            self.dep_checked.insert(issue.number);
        }
    }

    fn dep_persist_to_payload(&mut self) {
        self.payload.blocked_by = self.dep_checked.iter().copied().collect();
    }

    pub fn step(&self) -> IssueWizardStep {
        self.step
    }

    pub fn payload(&self) -> &IssueCreationPayload {
        &self.payload
    }

    pub fn payload_mut(&mut self) -> &mut IssueCreationPayload {
        &mut self.payload
    }

    /// Fields rendered on the current step, in tab-cycle order.
    pub fn step_fields(&self) -> Vec<FieldId> {
        match self.step {
            IssueWizardStep::BasicInfo => vec![FieldId::Title, FieldId::Overview],
            IssueWizardStep::DorFields => match self.payload.issue_type {
                IssueType::Feature => vec![
                    FieldId::ExpectedBehavior,
                    FieldId::AcceptanceCriteria,
                    FieldId::FilesToModify,
                    FieldId::TestHints,
                ],
                IssueType::Bug => vec![
                    FieldId::CurrentBehavior,
                    FieldId::StepsToReproduce,
                    FieldId::ExpectedBehavior,
                    FieldId::AcceptanceCriteria,
                    FieldId::FilesToModify,
                    FieldId::TestHints,
                ],
            },
            _ => Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn focused_field(&self) -> Option<FieldId> {
        let step_fields = self.step_fields();
        if step_fields.is_empty() {
            None
        } else {
            step_fields.get(self.fields.focus()).copied()
        }
    }

    /// Current text for `field`. Reads from the live textarea when the
    /// field belongs to the current step; otherwise falls back to the
    /// last-synced value on `payload`. Validation, preview rendering,
    /// and AI prompts all flow through this single reader.
    pub fn field_text(&self, field: FieldId) -> String {
        let step_fields = self.step_fields();
        if let Some(idx) = step_fields.iter().position(|f| *f == field)
            && let Some(ta_field) = self.fields.get(idx)
        {
            return ta_field.text();
        }
        field.payload_ref(&self.payload).to_string()
    }

    /// Flush live textarea content into the payload strings. Called at
    /// every seam where `payload` is consumed: step transitions
    /// (`try_advance`, `retreat`, `jump_to`), paste insertion, and the
    /// AI/create launch hooks.
    fn sync_fields_into_payload(&mut self) {
        let step_fields = self.step_fields();
        for (idx, field_id) in step_fields.iter().enumerate() {
            let Some(ta_field) = self.fields.get(idx) else {
                continue;
            };
            let text = ta_field.text();
            *field_id.payload_ref_mut(&mut self.payload) = text;
        }
    }

    /// Reconstruct `self.fields` for the current step, seeding each
    /// textarea with the existing payload value so Back-then-Forward
    /// recovers what the user typed earlier. Called after every step
    /// change.
    fn rebuild_fields_for_step(&mut self) {
        let step_fields = self.step_fields();
        if step_fields.is_empty() {
            self.fields = WizardFields::empty();
            return;
        }
        let mut new_fields: Vec<TextAreaField> = Vec::with_capacity(step_fields.len());
        for field_id in step_fields {
            let mut ta = if matches!(field_id, FieldId::Title) {
                TextAreaField::single_line()
            } else {
                TextAreaField::multi_line()
            };
            let initial = field_id.payload_ref(&self.payload);
            if !initial.is_empty() {
                ta.set_text(initial);
            }
            new_fields.push(ta);
        }
        self.fields = WizardFields::new(new_fields);
    }

    /// Paint focused/unfocused border styles onto each textarea's
    /// internal `Block`. Called from `Screen::draw` (the only mutable
    /// entry point) so `draw_impl` can stay `&self`.
    pub(super) fn refresh_field_blocks(&mut self) {
        let step_fields = self.step_fields();
        let focused_idx = self.fields.focus();
        for (i, field_id) in step_fields.iter().enumerate() {
            let focused = i == focused_idx;
            let border_style = if focused {
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::DIM)
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(field_id.label());
            if let Some(ta) = self.fields.get_mut(i) {
                ta.area_mut().set_block(block);
            }
        }
    }

    /// Validate that the current step is allowed to advance. Reads
    /// live textarea content via `field_text` so the footer error
    /// updates as the user types (no wait for a sync boundary).
    pub fn validation_error(&self) -> Option<&'static str> {
        match self.step {
            IssueWizardStep::BasicInfo => {
                if self.field_text(FieldId::Title).trim().is_empty() {
                    Some("Title is required")
                } else if self.field_text(FieldId::Overview).trim().is_empty() {
                    Some("Overview is required")
                } else {
                    None
                }
            }
            IssueWizardStep::DorFields => {
                if self
                    .field_text(FieldId::AcceptanceCriteria)
                    .trim()
                    .is_empty()
                {
                    Some("Acceptance Criteria is required")
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Try to advance. Returns true on success, false if validation
    /// blocked the move.
    pub fn try_advance(&mut self) -> bool {
        // Sync BEFORE validation so payload reflects the live textarea
        // content, then validation reads the canonical source.
        self.sync_fields_into_payload();
        if self.validation_error().is_some() {
            return false;
        }
        if matches!(self.step, IssueWizardStep::Dependencies) {
            self.dep_persist_to_payload();
        }
        if let Some(next) = self.step.next() {
            self.step = next;
            self.rebuild_fields_for_step();
            true
        } else {
            false
        }
    }

    pub fn retreat(&mut self) -> bool {
        self.sync_fields_into_payload();
        if let Some(prev) = self.step.previous() {
            self.step = prev;
            self.rebuild_fields_for_step();
            true
        } else {
            false
        }
    }

    fn handle_type_select(&mut self, code: KeyCode) -> ScreenAction {
        match code {
            KeyCode::Left => self.payload.issue_type = IssueType::Feature,
            KeyCode::Right => self.payload.issue_type = IssueType::Bug,
            KeyCode::Char('h') => self.payload.issue_type = IssueType::Feature,
            KeyCode::Char('l') => self.payload.issue_type = IssueType::Bug,
            _ => {}
        }
        ScreenAction::None
    }
}

impl Default for IssueWizardScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapProvider for IssueWizardScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Issue Wizard",
            bindings: vec![
                KeyBinding {
                    key: "Enter",
                    description: "Next step (validates required fields)",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Previous step (or close)",
                },
                KeyBinding {
                    key: "Tab",
                    description: "Cycle focus between fields",
                },
                KeyBinding {
                    key: "Shift+Enter",
                    description: "Newline in multi-line fields",
                },
                KeyBinding {
                    key: "←/→",
                    description: "TypeSelect: Feature ↔ Bug",
                },
            ],
        }]
    }
}

impl Screen for IssueWizardScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        // Bracketed paste (Cmd+V in the terminal) arrives as Event::Paste.
        // Route it to the focused field regardless of step, so every
        // text surface in the wizard accepts clipboard content.
        if let Event::Paste(text) = event {
            self.paste_text_into_focused(text);
            return ScreenAction::None;
        }

        let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            modifiers,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };

        // Ctrl+V: explicit clipboard read (covers images on the clipboard,
        // which bracketed paste can't carry).
        if modifiers.contains(KeyModifiers::CONTROL) && *code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return ScreenAction::None;
        }

        // Step-specific routing.
        match self.step {
            IssueWizardStep::TypeSelect => match code {
                KeyCode::Esc => {
                    self.retreat();
                    return ScreenAction::None;
                }
                KeyCode::Enter => {
                    self.try_advance();
                    return ScreenAction::None;
                }
                _ => return self.handle_type_select(*code),
            },
            IssueWizardStep::BasicInfo | IssueWizardStep::DorFields => match (code, *modifiers) {
                (KeyCode::Esc, _) => {
                    self.retreat();
                }
                (KeyCode::Tab, _) => {
                    self.fields.focus_next();
                }
                (KeyCode::BackTab, _) => {
                    self.fields.focus_prev();
                }
                (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
                    // Shift+Enter inserts a newline on multi-line fields
                    // (the Title single-line field filters it out).
                    if let Some(field) = self.fields.focused_mut()
                        && !field.is_single_line()
                    {
                        field.area_mut().insert_newline();
                    }
                }
                (KeyCode::Enter, _) => {
                    self.try_advance();
                }
                _ => {
                    // Everything else — printable Chars, Backspace,
                    // arrow keys, Home/End, Ctrl+Left/Right (word jump),
                    // Ctrl+W (delete word back), Ctrl+Z/Y (undo/redo),
                    // Shift+arrow selection — delegates to TextArea.
                    if let Some(field) = self.fields.focused_mut() {
                        field.area_mut().input(event.clone());
                    }
                }
            },
            IssueWizardStep::AiReview => match code {
                KeyCode::Esc => {
                    self.retreat();
                }
                KeyCode::Char('r') => {
                    // "Revise" — jump back to BasicInfo so the user can edit.
                    self.jump_to(IssueWizardStep::BasicInfo);
                }
                KeyCode::Char('s') => {
                    // Skip the review and head straight to Preview.
                    self.jump_to(IssueWizardStep::Preview);
                }
                KeyCode::Char('R') if self.review_error.is_some() => {
                    // Retry on error.
                    self.begin_ai_review();
                }
                KeyCode::Enter => {
                    if self.review_loading || self.review_error.is_some() {
                        // Block advance while loading or after an error
                        // (use 'R' to retry, 's' to skip).
                    } else {
                        self.try_advance();
                    }
                }
                _ => {}
            },
            IssueWizardStep::Preview => match code {
                KeyCode::Esc => {
                    self.retreat();
                }
                KeyCode::Enter => {
                    // The dispatch layer's `tick_wizard_step_hooks` queues
                    // `TuiCommand::CreateIssue` when the wizard transitions
                    // to `Creating`.
                    self.begin_create();
                }
                _ => {}
            },
            IssueWizardStep::Failed => match code {
                KeyCode::Esc => {
                    self.step = IssueWizardStep::Preview;
                    self.create_error = None;
                }
                KeyCode::Char('r') => {
                    self.create_error = None;
                    self.begin_create();
                }
                _ => {}
            },
            IssueWizardStep::Complete => match code {
                KeyCode::Enter => {
                    self.reset_for_another();
                }
                KeyCode::Esc => {
                    return ScreenAction::Pop;
                }
                _ => {}
            },
            IssueWizardStep::Creating => {
                // No-op while in flight — the data event triggers the
                // transition to Complete or Failed.
            }
            IssueWizardStep::Dependencies => match code {
                KeyCode::Esc => {
                    self.retreat();
                }
                KeyCode::Char(' ') => self.dep_toggle_selected(),
                KeyCode::Char('j') | KeyCode::Down => {
                    if let Some(issues) = self.dep_issues.as_ref()
                        && !issues.is_empty()
                        && self.dep_selected + 1 < issues.len()
                    {
                        self.dep_selected += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.dep_selected = self.dep_selected.saturating_sub(1);
                }
                KeyCode::Enter => {
                    self.try_advance();
                }
                _ => {}
            },
            _ => match code {
                KeyCode::Esc => {
                    if self.step.is_first() {
                        return ScreenAction::Pop;
                    }
                    self.retreat();
                }
                KeyCode::Enter => {
                    self.try_advance();
                }
                _ => {}
            },
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.fields.refresh_focus_styles();
        self.refresh_field_blocks();
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        // Form steps capture printable characters as field input — request
        // Insert mode so the global `q` shortcut doesn't steal them.
        if matches!(
            self.step,
            IssueWizardStep::BasicInfo | IssueWizardStep::DorFields
        ) {
            Some(InputMode::Insert)
        } else {
            Some(InputMode::Normal)
        }
    }
}

impl IssueWizardScreen {
    /// Step-aware advance hook used by the dispatch layer to know whether
    /// an entry into `Dependencies` should kick off a fetch. Guards
    /// against re-enqueuing while a previous fetch is in flight (would
    /// otherwise spawn a duplicate `gh` subprocess on every keypress).
    pub fn entered_dependencies_step(&self) -> bool {
        matches!(self.step, IssueWizardStep::Dependencies)
            && self.dep_issues.is_none()
            && !self.dep_loading
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
    use crossterm::event::KeyModifiers;

    fn type_string(s: &mut IssueWizardScreen, text: &str) {
        for c in text.chars() {
            s.handle_input(&key_event(KeyCode::Char(c)), InputMode::Insert);
        }
    }

    #[test]
    fn new_starts_at_context_step() {
        let s = IssueWizardScreen::new();
        assert_eq!(s.step(), IssueWizardStep::Context);
    }

    #[test]
    fn step_index_is_one_based() {
        assert_eq!(IssueWizardStep::Context.index(), 1);
        assert_eq!(IssueWizardStep::TypeSelect.index(), 2);
        assert_eq!(IssueWizardStep::Failed.index(), 10);
    }

    #[test]
    fn step_total_is_ten() {
        assert_eq!(IssueWizardStep::total(), 10);
    }

    #[test]
    fn enter_advances_step() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(s.step(), IssueWizardStep::TypeSelect);
    }

    #[test]
    fn esc_on_first_step_returns_pop() {
        let mut s = IssueWizardScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    // ---- TypeSelect ----

    #[test]
    fn type_select_default_is_feature() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(s.payload().issue_type, IssueType::Feature);
    }

    #[test]
    fn type_select_right_arrow_picks_bug() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Right), InputMode::Normal);
        assert_eq!(s.payload().issue_type, IssueType::Bug);
    }

    #[test]
    fn type_select_left_arrow_picks_feature() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Right), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Left), InputMode::Normal);
        assert_eq!(s.payload().issue_type, IssueType::Feature);
    }

    #[test]
    fn type_select_h_l_pick_feature_or_bug() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
        assert_eq!(s.payload().issue_type, IssueType::Bug);
        s.handle_input(&key_event(KeyCode::Char('h')), InputMode::Normal);
        assert_eq!(s.payload().issue_type, IssueType::Feature);
    }

    // ---- BasicInfo ----

    fn at_basic_info() -> IssueWizardScreen {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal); // → TypeSelect
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal); // → BasicInfo
        s
    }

    #[test]
    fn basic_info_chars_append_to_title() {
        let mut s = at_basic_info();
        type_string(&mut s, "hello");
        assert_eq!(s.field_text(FieldId::Title), "hello");
    }

    /// Post-migration invariant: typed text lives in the textarea until
    /// a step transition (advance / Esc / Ctrl+V) syncs it into the
    /// payload. The previous `String`-buffer wizard pushed chars
    /// directly into `payload.title` — keeping those two paths distinct
    /// is the whole point of #447 (so validation, preview, and AI
    /// prompts read from one source of truth — the live textarea).
    #[test]
    fn basic_info_typed_text_not_in_payload_until_sync() {
        let mut s = at_basic_info();
        type_string(&mut s, "hello");
        assert!(
            s.payload().title.is_empty(),
            "payload.title should stay empty until a sync boundary"
        );
    }

    #[test]
    fn basic_info_tab_cycles_to_overview() {
        let mut s = at_basic_info();
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        assert_eq!(s.focused_field(), Some(FieldId::Overview));
        type_string(&mut s, "x");
        assert_eq!(s.field_text(FieldId::Overview), "x");
        assert_eq!(s.field_text(FieldId::Title), "");
    }

    #[test]
    fn basic_info_backspace_removes_last_char() {
        let mut s = at_basic_info();
        type_string(&mut s, "hi");
        s.handle_input(&key_event(KeyCode::Backspace), InputMode::Insert);
        assert_eq!(s.field_text(FieldId::Title), "h");
    }

    #[test]
    fn basic_info_advance_blocked_when_title_empty() {
        let mut s = at_basic_info();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::BasicInfo);
    }

    #[test]
    fn basic_info_advance_blocked_when_overview_empty() {
        let mut s = at_basic_info();
        type_string(&mut s, "title");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::BasicInfo);
    }

    #[test]
    fn basic_info_advance_succeeds_when_both_filled() {
        let mut s = at_basic_info();
        type_string(&mut s, "title");
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        type_string(&mut s, "overview");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::DorFields);
    }

    #[test]
    fn basic_info_shift_enter_inserts_newline_in_overview() {
        let mut s = at_basic_info();
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        type_string(&mut s, "a");
        s.handle_input(
            &key_event_with_modifiers(KeyCode::Enter, KeyModifiers::SHIFT),
            InputMode::Insert,
        );
        type_string(&mut s, "b");
        assert_eq!(s.field_text(FieldId::Overview), "a\nb");
    }

    #[test]
    fn basic_info_shift_enter_does_not_insert_newline_in_title() {
        let mut s = at_basic_info();
        type_string(&mut s, "a");
        s.handle_input(
            &key_event_with_modifiers(KeyCode::Enter, KeyModifiers::SHIFT),
            InputMode::Insert,
        );
        type_string(&mut s, "b");
        assert_eq!(s.field_text(FieldId::Title), "ab");
    }

    // ---- DorFields ----

    fn at_dor_fields(issue_type: IssueType) -> IssueWizardScreen {
        let mut s = at_basic_info();
        // `issue_type` is a payload-only field (not a textarea), so
        // setting it via `payload_mut` stays correct.
        s.payload_mut().issue_type = issue_type;
        // Title + Overview now live in textareas; seed them via the
        // public input path so try_advance sees populated content.
        type_string(&mut s, "t");
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        type_string(&mut s, "o");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        s
    }

    #[test]
    fn dor_fields_feature_has_four_fields() {
        let s = at_dor_fields(IssueType::Feature);
        assert_eq!(s.step(), IssueWizardStep::DorFields);
        assert_eq!(s.step_fields().len(), 4);
        assert!(!s.step_fields().contains(&FieldId::CurrentBehavior));
    }

    #[test]
    fn dor_fields_bug_has_six_fields() {
        let s = at_dor_fields(IssueType::Bug);
        assert_eq!(s.step_fields().len(), 6);
        assert!(s.step_fields().contains(&FieldId::CurrentBehavior));
        assert!(s.step_fields().contains(&FieldId::StepsToReproduce));
    }

    #[test]
    fn dor_fields_tab_cycles_through_all_fields() {
        let mut s = at_dor_fields(IssueType::Feature);
        let total = s.step_fields().len();
        let initial = s.focused_field();
        for _ in 0..total {
            s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        }
        assert_eq!(s.focused_field(), initial);
    }

    #[test]
    fn dor_fields_back_tab_cycles_backwards() {
        let mut s = at_dor_fields(IssueType::Feature);
        let initial = s.focused_field();
        s.handle_input(&key_event(KeyCode::BackTab), InputMode::Insert);
        let prev = s.focused_field();
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        assert_eq!(s.focused_field(), initial);
        assert_ne!(initial, prev);
    }

    #[test]
    fn dor_fields_advance_blocked_when_acceptance_empty() {
        let mut s = at_dor_fields(IssueType::Feature);
        type_string(&mut s, "expected");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::DorFields);
    }

    #[test]
    fn dor_fields_advance_succeeds_with_acceptance_filled() {
        let mut s = at_dor_fields(IssueType::Feature);
        // Move to acceptance criteria field
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        assert_eq!(s.focused_field(), Some(FieldId::AcceptanceCriteria));
        type_string(&mut s, "criteria");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::Dependencies);
    }

    #[test]
    fn esc_in_basic_info_returns_to_type_select() {
        let mut s = at_basic_info();
        s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::TypeSelect);
    }

    // ---- Insert input mode for capturing chars ----

    #[test]
    fn basic_info_desires_insert_mode() {
        let s = at_basic_info();
        assert_eq!(s.desired_input_mode(), Some(InputMode::Insert));
    }

    #[test]
    fn type_select_desires_normal_mode() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(s.desired_input_mode(), Some(InputMode::Normal));
    }

    // ---- Paste handling (Event::Paste + Ctrl+V) ----

    struct FakeClipboard(std::sync::Mutex<super::ClipboardContent>);

    impl FakeClipboard {
        fn new(content: super::ClipboardContent) -> Self {
            Self(std::sync::Mutex::new(content))
        }
    }

    impl super::ClipboardProvider for FakeClipboard {
        fn read(&self) -> super::ClipboardContent {
            let mut guard = self.0.lock().unwrap();
            std::mem::replace(&mut *guard, super::ClipboardContent::Empty)
        }
    }

    fn basic_info_wizard(clip: super::ClipboardContent) -> IssueWizardScreen {
        let mut s = IssueWizardScreen::with_clipboard(Box::new(FakeClipboard::new(clip)));
        s.try_advance(); // Context → TypeSelect
        s.try_advance(); // TypeSelect → BasicInfo
        s
    }

    fn paste_event(text: &str) -> crossterm::event::Event {
        crossterm::event::Event::Paste(text.to_string())
    }

    #[test]
    fn bracketed_paste_appends_to_title_when_title_focused() {
        let mut s = basic_info_wizard(super::ClipboardContent::Empty);
        type_string(&mut s, "hi ");
        s.handle_input(&paste_event("from clipboard"), InputMode::Insert);
        assert_eq!(s.payload().title, "hi from clipboard");
    }

    #[test]
    fn bracketed_paste_replaces_newlines_with_spaces_in_single_line_title() {
        let mut s = basic_info_wizard(super::ClipboardContent::Empty);
        s.handle_input(&paste_event("line1\nline2"), InputMode::Insert);
        assert_eq!(s.payload().title, "line1 line2");
    }

    #[test]
    fn bracketed_paste_preserves_newlines_in_overview() {
        let mut s = basic_info_wizard(super::ClipboardContent::Empty);
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert); // focus Overview
        s.handle_input(&paste_event("line1\nline2"), InputMode::Insert);
        assert_eq!(s.payload().overview, "line1\nline2");
    }

    #[test]
    fn bracketed_paste_strips_ansi_escape_sequences() {
        let mut s = basic_info_wizard(super::ClipboardContent::Empty);
        s.handle_input(&paste_event("\x1b[31mred\x1b[0m"), InputMode::Insert);
        assert!(!s.payload().title.contains('\x1b'));
    }

    #[test]
    fn bracketed_paste_does_not_trigger_try_advance() {
        let mut s = basic_info_wizard(super::ClipboardContent::Empty);
        s.handle_input(&paste_event("title\n\nmore"), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::BasicInfo);
    }

    #[test]
    fn ctrl_v_text_clipboard_pastes_into_focused_field() {
        let mut s = basic_info_wizard(super::ClipboardContent::Text("clipboard text".to_string()));
        s.handle_input(
            &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
            InputMode::Insert,
        );
        assert_eq!(s.payload().title, "clipboard text");
    }

    #[test]
    fn ctrl_v_image_clipboard_adds_to_image_paths() {
        let tmp = std::env::temp_dir().join("wizard-test.png");
        let mut s = basic_info_wizard(super::ClipboardContent::Image(tmp.clone()));
        s.handle_input(
            &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
            InputMode::Insert,
        );
        assert_eq!(
            s.payload().image_paths,
            vec![tmp.to_string_lossy().to_string()]
        );
        assert!(s.payload().title.is_empty());
    }

    #[test]
    fn ctrl_v_on_empty_clipboard_is_noop() {
        let mut s = basic_info_wizard(super::ClipboardContent::Empty);
        s.handle_input(
            &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
            InputMode::Insert,
        );
        assert!(s.payload().title.is_empty());
        assert!(s.payload().image_paths.is_empty());
    }

    #[test]
    fn render_body_includes_attachments_section_when_images_present() {
        let mut p = IssueCreationPayload::default();
        p.title = "t".into();
        p.overview = "o".into();
        p.expected_behavior = "e".into();
        p.acceptance_criteria = "a".into();
        p.image_paths = vec!["/tmp/a.png".into(), "/tmp/b.png".into()];
        let body = render_body_markdown(&p);
        assert!(body.contains("## Attachments"));
        assert!(body.contains("[Attached image: /tmp/a.png]"));
        assert!(body.contains("[Attached image: /tmp/b.png]"));
    }

    #[test]
    fn render_body_omits_attachments_section_when_empty() {
        let mut p = IssueCreationPayload::default();
        p.title = "t".into();
        p.overview = "o".into();
        p.expected_behavior = "e".into();
        p.acceptance_criteria = "a".into();
        let body = render_body_markdown(&p);
        assert!(!body.contains("## Attachments"));
    }

    // ---- #295 Dependencies step ----

    fn make_open_issue(number: u64) -> crate::provider::github::types::GhIssue {
        crate::provider::github::types::GhIssue {
            number,
            title: format!("Issue #{}", number),
            body: String::new(),
            labels: vec![],
            state: "open".to_string(),
            html_url: String::new(),
            milestone: None,
            assignees: vec![],
        }
    }

    fn at_dependencies() -> IssueWizardScreen {
        let mut s = at_dor_fields(IssueType::Feature);
        // Move to acceptance criteria field, fill it, advance to Dependencies.
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        type_string(&mut s, "criteria");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::Dependencies);
        s
    }

    #[test]
    fn entering_dependencies_marks_fetch_required() {
        let s = at_dependencies();
        assert!(s.entered_dependencies_step());
    }

    #[test]
    fn begin_dependency_fetch_seeds_loading_flag() {
        let mut s = at_dependencies();
        s.begin_dependency_fetch();
        assert!(s.dep_loading());
    }

    #[test]
    fn apply_dep_issues_filters_to_open_only() {
        let mut s = at_dependencies();
        s.begin_dependency_fetch();
        let mut closed = make_open_issue(99);
        closed.state = "closed".into();
        s.apply_dep_issues(vec![make_open_issue(10), closed, make_open_issue(11)]);
        assert!(!s.dep_loading());
        assert_eq!(s.dep_issues().unwrap().len(), 2);
    }

    #[test]
    fn space_toggles_checkbox_on_selected() {
        let mut s = at_dependencies();
        s.begin_dependency_fetch();
        s.apply_dep_issues(vec![make_open_issue(10), make_open_issue(11)]);
        s.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(s.dep_is_checked(10));
        s.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(!s.dep_is_checked(10));
    }

    #[test]
    fn j_navigates_dependency_list() {
        let mut s = at_dependencies();
        s.begin_dependency_fetch();
        s.apply_dep_issues(vec![make_open_issue(10), make_open_issue(11)]);
        s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(s.dep_selected(), 1);
    }

    #[test]
    fn j_clamps_at_end_of_list() {
        let mut s = at_dependencies();
        s.begin_dependency_fetch();
        s.apply_dep_issues(vec![make_open_issue(10)]);
        for _ in 0..5 {
            s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(s.dep_selected(), 0);
    }

    #[test]
    fn k_does_not_underflow() {
        let mut s = at_dependencies();
        s.begin_dependency_fetch();
        s.apply_dep_issues(vec![make_open_issue(10)]);
        s.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(s.dep_selected(), 0);
    }

    #[test]
    fn enter_persists_checked_set_to_payload_blocked_by() {
        let mut s = at_dependencies();
        s.begin_dependency_fetch();
        s.apply_dep_issues(vec![make_open_issue(10), make_open_issue(11)]);
        s.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(s.payload().blocked_by, vec![10, 11]);
        assert_eq!(s.step(), IssueWizardStep::AiReview);
    }

    #[test]
    fn esc_in_dependencies_step_retreats() {
        let mut s = at_dependencies();
        s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(s.step(), IssueWizardStep::DorFields);
    }

    // ---- #298 Preview / Creating / Complete / Failed ----

    fn at_preview() -> IssueWizardScreen {
        let mut s = IssueWizardScreen::new();
        s.payload_mut().title = "Add gauge widget".into();
        s.payload_mut().overview = "Render progress".into();
        s.payload_mut().expected_behavior = "Fills proportionally".into();
        s.payload_mut().acceptance_criteria = "- Renders 0..100".into();
        s.payload_mut().blocked_by = vec![10];
        // Jump directly for testing.
        s.jump_to(IssueWizardStep::Preview);
        s
    }

    #[test]
    fn render_body_includes_all_required_sections() {
        let s = at_preview();
        let body = s.render_body_markdown();
        for section in [
            "## Overview",
            "## Expected Behavior",
            "## Acceptance Criteria",
            "## Blocked By",
            "## Definition of Done",
        ] {
            assert!(body.contains(section), "missing section {section}");
        }
        assert!(body.contains("- #10"));
    }

    #[test]
    fn render_body_blocked_by_none_when_empty() {
        let mut s = at_preview();
        s.payload_mut().blocked_by.clear();
        let body = s.render_body_markdown();
        assert!(body.contains("## Blocked By\n\n- None"));
    }

    #[test]
    fn render_body_definition_of_done_converts_bullets_to_checklist() {
        let mut s = at_preview();
        s.payload_mut().acceptance_criteria = "- A\n- B\n* C".into();
        let body = s.render_body_markdown();
        assert!(body.contains("- [ ] A"));
        assert!(body.contains("- [ ] B"));
        assert!(body.contains("- [ ] C"));
    }

    #[test]
    fn render_body_omits_bug_only_sections_for_feature() {
        let s = at_preview();
        let body = s.render_body_markdown();
        assert!(!body.contains("## Current Behavior"));
        assert!(!body.contains("## Steps to Reproduce"));
    }

    #[test]
    fn render_body_includes_bug_only_sections_when_filled() {
        let mut s = at_preview();
        s.payload_mut().issue_type = IssueType::Bug;
        s.payload_mut().current_behavior = "Crashes".into();
        s.payload_mut().steps_to_reproduce = "open then crash".into();
        let body = s.render_body_markdown();
        assert!(body.contains("## Current Behavior"));
        assert!(body.contains("## Steps to Reproduce"));
    }

    #[test]
    fn render_labels_includes_maestro_ready_and_type_label() {
        let s = at_preview();
        let labels = s.render_labels();
        assert!(labels.contains(&"maestro:ready".to_string()));
        assert!(labels.contains(&"enhancement".to_string()));
    }

    #[test]
    fn render_labels_uses_bug_for_bug_type() {
        let mut s = at_preview();
        s.payload_mut().issue_type = IssueType::Bug;
        assert!(s.render_labels().contains(&"bug".to_string()));
    }

    #[test]
    fn enter_on_preview_begins_create() {
        let mut s = at_preview();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert!(s.create_in_flight());
        assert_eq!(s.step(), IssueWizardStep::Creating);
    }

    #[test]
    fn finish_create_ok_transitions_to_complete() {
        let mut s = at_preview();
        s.begin_create();
        s.finish_create(Ok(42));
        assert!(!s.create_in_flight());
        assert_eq!(s.created_issue_number(), Some(42));
        assert_eq!(s.step(), IssueWizardStep::Complete);
    }

    #[test]
    fn finish_create_err_transitions_to_failed() {
        let mut s = at_preview();
        s.begin_create();
        s.finish_create(Err(anyhow::anyhow!("API down")));
        assert_eq!(s.step(), IssueWizardStep::Failed);
        assert_eq!(s.create_error(), Some("API down"));
    }

    #[test]
    fn enter_on_complete_resets_for_another_issue() {
        let mut s = at_preview();
        s.begin_create();
        s.finish_create(Ok(42));
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(s.step(), IssueWizardStep::Context);
        assert!(s.payload().title.is_empty());
    }

    #[test]
    fn esc_on_complete_pops_back_to_landing() {
        let mut s = at_preview();
        s.begin_create();
        s.finish_create(Ok(42));
        let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn r_on_failed_retries_create() {
        let mut s = at_preview();
        s.begin_create();
        s.finish_create(Err(anyhow::anyhow!("boom")));
        s.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
        assert_eq!(s.step(), IssueWizardStep::Creating);
        assert!(s.create_in_flight());
        assert!(s.create_error().is_none());
    }

    #[test]
    fn pre_seeded_blocked_by_shows_as_checked_after_fetch() {
        // #326 path: payload arrived with blocked_by pre-filled.
        let mut s = at_dependencies();
        s.payload_mut().blocked_by = vec![11];
        s.begin_dependency_fetch();
        s.apply_dep_issues(vec![make_open_issue(10), make_open_issue(11)]);
        assert!(s.dep_is_checked(11));
        assert!(!s.dep_is_checked(10));
    }
}
