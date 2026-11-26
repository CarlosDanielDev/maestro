pub mod ai_improve;
pub mod ai_review;
mod deps;
mod draw;
mod draw_ai_review;
mod draw_deps;
mod draw_diff;
mod draw_gate;
mod gate;
mod improve_sections;
pub mod lint;
mod prompt_common;
pub mod types;
mod validation;

pub use ai_improve::{build_improve_prompt, parse_improve_response};
pub use ai_review::build_review_prompt;
pub use improve_sections::Decision;
pub use types::{IssueCreationPayload, IssueType, IssueWizardStep};

use super::prompt_input::{ClipboardContent, ClipboardProvider, SystemClipboard};
use super::wizard_fields::{TextAreaField, WizardFields};
use super::{Screen, ScreenAction};
use crate::tui::ds::armed::ArmedEnter;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
};
use std::collections::BTreeSet;
use std::time::Instant;

/// Identifier of a focusable text field inside a wizard step. Stable across
/// re-renders so the focused field survives a redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    /// Every text field, in `draw_diff::diff_fields` order.
    pub const ALL: [Self; 8] = [
        Self::Title,
        Self::Overview,
        Self::ExpectedBehavior,
        Self::CurrentBehavior,
        Self::StepsToReproduce,
        Self::AcceptanceCriteria,
        Self::FilesToModify,
        Self::TestHints,
    ];

    /// Whether `render_body_markdown` writes this field for `kind`: the
    /// bug-only fields are dropped from a Feature.
    pub const fn is_written_for(&self, kind: IssueType) -> bool {
        match self {
            Self::CurrentBehavior | Self::StepsToReproduce => matches!(kind, IssueType::Bug),
            _ => true,
        }
    }

    /// The step whose form holds this field.
    pub const fn step(&self) -> IssueWizardStep {
        match self {
            Self::Title | Self::Overview => IssueWizardStep::BasicInfo,
            _ => IssueWizardStep::DorFields,
        }
    }

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

    pub(super) fn payload_ref<'a>(&self, p: &'a IssueCreationPayload) -> &'a str {
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

    pub(super) fn payload_ref_mut<'a>(&self, p: &'a mut IssueCreationPayload) -> &'a mut String {
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

/// How long a second Esc on Basics still closes a typed draft (the same
/// window as `ds::confirm::HoldToConfirm`'s second press).
pub const LEAVE_WINDOW: std::time::Duration = crate::tui::ds::confirm::WINDOW;

pub struct IssueWizardScreen {
    step: IssueWizardStep,
    payload: IssueCreationPayload,
    /// Text fields for the current step. Rebuilt whenever the step
    /// changes so BasicInfo owns Title+Overview textareas, DorFields
    /// owns its 4 or 6 textareas, and non-text steps hold `empty()`.
    pub(super) fields: WizardFields,
    /// #1282: the Basics Type row (between Title and Overview) has focus.
    /// It is a NORMAL-mode stop, so letters pick the type, not text.
    pub(super) type_focused: bool,
    /// #1282: fields the user has left or tried to advance past. Only these
    /// show their error on the border; the checklist shows every error.
    pub(super) touched: BTreeSet<FieldId>,
    /// #295 Dependencies step state.
    pub(super) dep_issues: Option<Vec<crate::provider::types::Issue>>,
    pub(super) dep_loading: bool,
    pub(super) dep_selected: usize,
    pub(super) dep_checked: BTreeSet<u64>,
    /// #1282: the Blocked-by filter (`/`), over number, title and labels.
    /// It narrows the list only; `dep_checked` is never touched by it.
    pub(super) dep_filter: TextAreaField,
    pub(super) dep_filter_editing: bool,
    /// #296 AI Review step state.
    pub(super) review_loading: bool,
    pub(super) review_text: Option<String>,
    pub(super) review_error: Option<String>,
    /// #1454: `r` asked for a review; the dispatch hook queues the one
    /// `LaunchAiReview` and `begin_ai_review` clears it. Entering the step
    /// never sets it, so no paid call starts unasked.
    pub(super) review_requested: bool,
    /// #450 AI Improve sub-state (layered on top of AiReview). When a
    /// candidate is present the draw layer swaps the review-text view
    /// for the diff view; when loading, the spinner renders; when
    /// error, the error banner renders. All exclusive with each other.
    pub(super) improve_loading: bool,
    pub(super) improve_candidate: Option<IssueCreationPayload>,
    pub(super) improve_error: Option<String>,
    /// Guards against `tick_wizard_step_hooks` dispatching a second
    /// `LaunchAiImprove` before the first one's result lands. Cleared
    /// when `apply_improve_result` fires.
    pub(super) improve_enqueued: bool,
    /// #1282: one decision per `FieldId::ALL` section of the candidate;
    /// only `Accepted` sections are merged.
    pub(super) decisions: [Decision; 8],
    /// #1282: the focused section (index into `FieldId::ALL`).
    pub(super) section_focus: usize,
    /// #1282: line offset inside the focused section's diff (↑/↓).
    pub(super) diff_scroll: u16,
    /// The diff view's last drawn (width, height), so ↓ stops when the end
    /// of the section is on screen.
    pub(super) diff_view: std::cell::Cell<(u16, u16)>,
    /// #1282: when the first Esc on Basics asked to drop a typed draft.
    pub(super) leave_armed_at: Option<Instant>,
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
    /// #455 Blocking modal surfaced when `create_issue` returns
    /// `CreateOutcome::Existed` — user must pick Edit or Cancel.
    pub(super) already_exists_modal: Option<AlreadyExistsModal>,
    /// #1282: the create gate on Preview: blocked while a `[TODO:` is left,
    /// then ⏎ arms and a second ⏎ within 3s creates.
    pub(super) create_gate: ArmedEnter,
    /// Tests inject the clock for the armed create; the app uses `now`.
    pub(super) clock: Option<Instant>,
    spinner_tick: usize,
    use_nerd_font: bool,
}

/// #455 Info needed to render the "issue already exists" blocking modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlreadyExistsModal {
    pub number: u64,
    pub state: String,
    pub title: String,
    /// 0 = Edit title (returns to BasicInfo), 1 = Cancel (closes wizard).
    pub focus: u8,
}

impl IssueWizardScreen {
    pub fn new() -> Self {
        Self::with_clipboard(Box::new(SystemClipboard))
    }

    pub fn with_clipboard(clipboard: Box<dyn ClipboardProvider>) -> Self {
        let mut s = Self {
            step: IssueWizardStep::default(),
            payload: IssueCreationPayload::new(),
            fields: WizardFields::empty(),
            type_focused: false,
            touched: BTreeSet::new(),
            dep_issues: None,
            dep_loading: false,
            dep_selected: 0,
            dep_checked: BTreeSet::new(),
            dep_filter: TextAreaField::single_line(),
            dep_filter_editing: false,
            review_loading: false,
            review_text: None,
            review_error: None,
            review_requested: false,
            improve_loading: false,
            improve_candidate: None,
            improve_error: None,
            improve_enqueued: false,
            decisions: [Decision::Pending; 8],
            section_focus: 0,
            diff_scroll: 0,
            diff_view: std::cell::Cell::new((0, 0)),
            leave_armed_at: None,
            clipboard,
            create_in_flight: false,
            create_enqueued: false,
            created_issue_number: None,
            create_error: None,
            already_exists_modal: None,
            create_gate: ArmedEnter::new(),
            clock: None,
            spinner_tick: 0,
            use_nerd_font: false,
        };
        s.rebuild_fields_for_step();
        s
    }

    pub fn set_spinner_context(&mut self, spinner_tick: usize, use_nerd_font: bool) {
        self.spinner_tick = spinner_tick;
        self.use_nerd_font = use_nerd_font;
    }

    pub(super) fn spinner_tick(&self) -> usize {
        self.spinner_tick
    }

    pub(super) fn use_nerd_font(&self) -> bool {
        self.use_nerd_font
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
                self.create_gate.finish(Ok(()));
            }
            Err(e) => {
                self.create_error = Some(e.to_string());
                self.step = IssueWizardStep::Failed;
                self.create_gate.finish(Err(e.to_string()));
            }
        }
    }

    /// #455 — surface the "already exists" blocking modal and clear
    /// the in-flight create flag. Leaves the wizard on the Creating step
    /// so the modal renders on top of the spinner frame.
    pub fn finish_create_already_exists(&mut self, number: u64, state: String, title: String) {
        self.create_in_flight = false;
        self.create_enqueued = false;
        self.create_gate.reset();
        self.already_exists_modal = Some(AlreadyExistsModal {
            number,
            state,
            title,
            focus: 0,
        });
    }

    pub fn already_exists_modal(&self) -> Option<&AlreadyExistsModal> {
        self.already_exists_modal.as_ref()
    }

    /// Cycle focus between Edit (0) and Cancel (1) in the already-exists modal.
    pub fn already_exists_modal_toggle_focus(&mut self) {
        if let Some(ref mut m) = self.already_exists_modal {
            m.focus = if m.focus == 0 { 1 } else { 0 };
        }
    }

    /// Commit the Edit action: dismiss the modal and return the user to
    /// the title step with their input preserved. The previous title is
    /// preloaded so the user can tweak it rather than retype.
    pub fn already_exists_modal_edit(&mut self) {
        self.already_exists_modal = None;
        self.step = IssueWizardStep::BasicInfo;
        self.create_error = None;
        self.rebuild_fields_for_step();
    }

    /// Commit the Cancel action: dismiss the modal. Caller closes wizard.
    pub fn already_exists_modal_cancel(&mut self) {
        self.already_exists_modal = None;
    }

    /// Reset to Basics with a fresh payload — used by the Complete
    /// step's "Enter to create another" action.
    pub fn reset_for_another(&mut self) {
        *self = Self::new();
    }

    // ---- #296 AI Review ----

    /// `r` before the first review or after a failed one: clear the error
    /// and ask for a review, so the dispatch hook (`tick_wizard_step_hooks`,
    /// the one queuer of `LaunchAiReview`) sees `ai_review_requested()` and
    /// queues exactly one call.
    pub fn retry_ai_review(&mut self) {
        self.review_error = None;
        self.review_text = None;
        self.review_loading = false;
        self.review_requested = true;
    }

    /// #1454: AI review before any call — no critique, no spinner, no error.
    pub fn review_idle(&self) -> bool {
        self.review_text.is_none()
            && !self.review_loading
            && self.review_error.is_none()
            && !self.review_requested
    }

    pub fn begin_ai_review(&mut self) {
        self.sync_fields_into_payload();
        self.review_requested = false;
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

    pub fn ai_review_requested(&self) -> bool {
        matches!(self.step, IssueWizardStep::AiReview)
            && self.review_requested
            && !self.review_loading
    }

    // ---- #450 AI Improve ----

    /// True when the user could legally press `i` to start the improve
    /// flow: review text is loaded without error, no improve call is
    /// already in flight, and no candidate/error modal is showing. Used
    /// by both `begin_improve` and the `i` key handler so the two can't
    /// drift apart.
    pub fn can_begin_improve(&self) -> bool {
        self.review_text.is_some()
            && !self.review_loading
            && self.review_error.is_none()
            && !self.improve_loading
            && self.improve_candidate.is_none()
            && self.improve_error.is_none()
    }

    /// True when ANY improve sub-state is active (loading, candidate
    /// diff, or error banner). Used to block `Enter`-advance and to
    /// route `Esc` to the sub-state's dismissal instead of the normal
    /// retreat-to-Dependencies.
    pub fn improve_sub_state_active(&self) -> bool {
        self.improve_loading || self.improve_candidate.is_some() || self.improve_error.is_some()
    }

    /// Launch the improve flow if the state machine allows it. Silently
    /// no-ops when `can_begin_improve` fails.
    pub fn begin_improve(&mut self) {
        if !self.can_begin_improve() {
            return;
        }
        self.sync_fields_into_payload();
        self.improve_loading = true;
        self.improve_candidate = None;
        self.improve_error = None;
    }

    /// Apply a result coming back from the background tokio task.
    /// `Ok(candidate)` shows the diff view; `Err(msg)` shows the error
    /// banner with retry/Esc affordances.
    pub fn apply_improve_result(&mut self, result: Result<IssueCreationPayload, String>) {
        self.improve_loading = false;
        self.improve_enqueued = false;
        match result {
            Ok(candidate) => {
                self.decisions = [Decision::Pending; 8];
                self.diff_scroll = 0;
                self.section_focus = improve_sections::changed(&self.payload, &candidate)
                    .first()
                    .copied()
                    .unwrap_or(0);
                self.improve_candidate = Some(candidate);
                self.improve_error = None;
            }
            Err(e) => {
                self.improve_candidate = None;
                self.improve_error = Some(e);
            }
        }
    }

    /// Drop the candidate and any error. The wizard's payload is unchanged.
    pub fn discard_improve(&mut self) {
        self.improve_candidate = None;
        self.improve_error = None;
        self.decisions = [Decision::Pending; 8];
    }

    /// True when `tick_wizard_step_hooks` should enqueue a new
    /// `LaunchAiImprove`. Flips to false after `mark_improve_enqueued`
    /// so the hook doesn't dispatch twice for the same request.
    pub fn improve_requested(&self) -> bool {
        self.improve_loading
            && self.improve_candidate.is_none()
            && self.improve_error.is_none()
            && !self.improve_enqueued
    }

    /// Latch set by the dispatch hook so subsequent ticks skip re-dispatch.
    pub fn mark_improve_enqueued(&mut self) {
        self.improve_enqueued = true;
    }

    pub fn improve_loading(&self) -> bool {
        self.improve_loading
    }

    pub fn improve_candidate(&self) -> Option<&IssueCreationPayload> {
        self.improve_candidate.as_ref()
    }

    pub fn improve_error(&self) -> Option<&str> {
        self.improve_error.as_deref()
    }

    /// Test helper — lets unit tests short-circuit to `AiReview` (or any
    /// other step) without walking the form's validation. Not exposed
    /// outside test builds.
    #[cfg(test)]
    pub fn set_step_for_tests(&mut self, step: IssueWizardStep) {
        self.step = step;
        self.rebuild_fields_for_step();
    }

    fn jump_to(&mut self, target: IssueWizardStep) {
        self.sync_fields_into_payload();
        self.step = target;
        self.rebuild_fields_for_step();
    }

    /// Basics: give the Type row focus (NORMAL mode).
    pub fn focus_type_row(&mut self) {
        if self.step == IssueWizardStep::BasicInfo {
            self.type_focused = true;
        }
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
    pub fn apply_dep_issues(&mut self, issues: Vec<crate::provider::types::Issue>) {
        // Filter to open issues only — closed issues can't block anything.
        let open: Vec<_> = issues.into_iter().filter(|i| i.state == "open").collect();
        self.dep_selected = self.dep_selected.min(open.len().saturating_sub(1));
        self.dep_issues = Some(open);
        self.dep_loading = false;
    }

    pub fn dep_issues(&self) -> Option<&[crate::provider::types::Issue]> {
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

    /// Space: toggle the highlighted row of the filtered list.
    fn dep_toggle_selected(&mut self) {
        let Some(&idx) = self.dep_visible().get(self.dep_selected) else {
            return;
        };
        let Some(issue) = self.dep_issues.as_ref().and_then(|i| i.get(idx)) else {
            return;
        };
        let number = issue.number;
        if !self.dep_checked.remove(&number) {
            self.dep_checked.insert(number);
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
        self.type_focused = false;
        if self.step == IssueWizardStep::Preview {
            self.refresh_gate();
        }
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

    /// The field borders are drawn by `draw_field` (`field_block`, with the
    /// error on the bottom border), so each textarea draws no block. The
    /// Type row hides the text cursor while it has focus.
    pub(super) fn refresh_field_blocks(&mut self) {
        for i in 0..self.step_fields().len() {
            if let Some(ta) = self.fields.get_mut(i) {
                ta.area_mut().remove_block();
            }
        }
        if self.type_focused
            && let Some(field) = self.fields.focused_mut()
        {
            field
                .area_mut()
                .set_cursor_style(Style::default().add_modifier(Modifier::HIDDEN));
        }
    }

    /// Try to advance. Returns true on success. When the step has errors,
    /// they are marked touched (so they show on the borders) and focus moves
    /// to the first invalid field.
    pub fn try_advance(&mut self) -> bool {
        // Sync BEFORE validation so payload reflects the live textarea
        // content, then validation reads the canonical source.
        self.sync_fields_into_payload();
        let errors = self.step_errors();
        if let Some(&(first, _)) = errors.first() {
            self.touched.extend(errors.iter().map(|(f, _)| *f));
            self.focus_field(first);
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

    /// Move to `field`'s step if needed and focus it.
    pub(super) fn focus_field(&mut self, field: FieldId) {
        if field.step() != self.step {
            self.jump_to(field.step());
        }
        self.type_focused = false;
        let Some(idx) = self.step_fields().iter().position(|f| *f == field) else {
            return;
        };
        for _ in 0..self.step_fields().len() {
            if self.fields.focus() == idx {
                break;
            }
            self.fields.focus_next();
        }
    }

    /// Tab on Basics and Details: Title → Type row → Overview → Title.
    /// The field left behind counts as touched.
    fn focus_step(&mut self, forward: bool) {
        let basics = self.step == IssueWizardStep::BasicInfo;
        if self.type_focused {
            self.type_focused = false;
            // The row sits between Title (0) and Overview (1).
            match (forward, self.fields.focus()) {
                (true, 0) => self.fields.focus_next(),
                (false, 1) => self.fields.focus_prev(),
                _ => {}
            }
            return;
        }
        if let Some(left) = self.step_fields().get(self.fields.focus()).copied() {
            self.touched.insert(left);
        }
        match (basics, forward, self.fields.focus()) {
            (true, true, 0) | (true, false, 1) => self.type_focused = true,
            (_, true, _) => self.fields.focus_next(),
            (_, false, _) => self.fields.focus_prev(),
        }
    }

    /// Type row keys: ←/f Feature, →/b Bug.
    fn handle_type_row(&mut self, code: KeyCode) {
        match code {
            KeyCode::Left | KeyCode::Char('f') => self.payload.issue_type = IssueType::Feature,
            KeyCode::Right | KeyCode::Char('b') => self.payload.issue_type = IssueType::Bug,
            _ => {}
        }
    }

    /// The clock for the armed create: injected in tests.
    pub(super) fn now(&self) -> Instant {
        self.clock.unwrap_or_else(Instant::now)
    }

    /// True when something was typed or picked: leaving would drop it.
    pub fn draft_is_dirty(&self) -> bool {
        FieldId::ALL
            .iter()
            .any(|f| !self.field_text(*f).trim().is_empty())
            || !self.payload.blocked_by.is_empty()
            || !self.payload.image_paths.is_empty()
    }

    /// True while a second Esc on Basics would close the wizard.
    pub fn leave_armed(&self) -> bool {
        self.leave_armed_at
            .is_some_and(|at| self.now().saturating_duration_since(at) <= LEAVE_WINDOW)
    }

    /// Esc on Basics: an empty draft closes at once; a typed one asks
    /// first (the screen says so) and closes on a second Esc within
    /// [`LEAVE_WINDOW`]. Returns true when the wizard should close.
    fn confirm_leave(&mut self) -> bool {
        if !self.draft_is_dirty() || self.leave_armed() {
            self.leave_armed_at = None;
            return true;
        }
        self.leave_armed_at = Some(self.now());
        false
    }

    #[cfg(test)]
    pub fn set_clock_for_test(&mut self, now: Instant) {
        self.clock = Some(now);
    }
}

impl Default for IssueWizardScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapProvider for IssueWizardScreen {
    /// Every other key is in the registry IssueWizard block (one source for
    /// Help); this group lists only the chord the registry cannot name.
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Issue Wizard",
            bindings: vec![KeyBinding {
                key: "Shift+Enter",
                description: "Newline in multi-line fields",
            }],
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

        // #455 — when the "already exists" modal is up, it swallows
        // all input until the user picks Edit or Cancel.
        if self.already_exists_modal.is_some() {
            match code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    self.already_exists_modal_toggle_focus();
                }
                KeyCode::Enter => {
                    let focus = self
                        .already_exists_modal
                        .as_ref()
                        .map(|m| m.focus)
                        .unwrap_or(0);
                    if focus == 0 {
                        self.already_exists_modal_edit();
                    } else {
                        self.already_exists_modal_cancel();
                        return ScreenAction::Pop;
                    }
                }
                KeyCode::Esc => {
                    self.already_exists_modal_cancel();
                    return ScreenAction::Pop;
                }
                _ => {}
            }
            return ScreenAction::None;
        }

        // Ctrl+V: explicit clipboard read (covers images on the clipboard,
        // which bracketed paste can't carry).
        if modifiers.contains(KeyModifiers::CONTROL) && *code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return ScreenAction::None;
        }

        // Any key but Esc keeps the draft and drops a pending "leave?".
        if *code != KeyCode::Esc {
            self.leave_armed_at = None;
        }

        // Step-specific routing.
        match self.step {
            IssueWizardStep::BasicInfo if *code == KeyCode::Esc => {
                if self.confirm_leave() {
                    return ScreenAction::Pop;
                }
            }
            IssueWizardStep::BasicInfo if self.type_focused => match code {
                KeyCode::Tab => self.focus_step(true),
                KeyCode::BackTab => self.focus_step(false),
                KeyCode::Enter => {
                    self.try_advance();
                }
                _ => self.handle_type_row(*code),
            },
            IssueWizardStep::BasicInfo | IssueWizardStep::DorFields => match (code, *modifiers) {
                (KeyCode::Esc, _) => {
                    self.retreat();
                }
                (KeyCode::Tab, _) => self.focus_step(true),
                (KeyCode::BackTab, _) => self.focus_step(false),
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
                        crate::tui::widgets::input::feed(field.area_mut(), event);
                    }
                }
            },
            IssueWizardStep::AiReview if self.improve_candidate.is_some() => {
                self.handle_improve_diff(*code)
            }
            IssueWizardStep::AiReview => match code {
                KeyCode::Esc if self.improve_error.is_some() => {
                    self.improve_error = None;
                }
                KeyCode::Esc => {
                    self.retreat();
                }
                KeyCode::Char('i') if self.can_begin_improve() => {
                    self.begin_improve();
                }
                // `r` has one meaning: run the AI call (paid) — the first
                // review, or again after a failure (#1454).
                KeyCode::Char('r') if self.improve_error.is_some() => {
                    self.improve_error = None;
                    self.begin_improve();
                }
                KeyCode::Char('r') if self.review_error.is_some() || self.review_idle() => {
                    self.retry_ai_review();
                }
                KeyCode::Char('e') => {
                    // "Revise" — jump back to BasicInfo so the user can edit.
                    self.jump_to(IssueWizardStep::BasicInfo);
                }
                KeyCode::Char('s') => {
                    self.jump_to(IssueWizardStep::Preview);
                }
                KeyCode::Enter
                    if !self.review_loading
                        && self.review_error.is_none()
                        && !self.improve_sub_state_active() =>
                {
                    self.try_advance();
                }
                // else: Enter is blocked while loading / error / improve sub-state;
                // use `r` (retry) or `s` (skip) to resolve.
                _ => {}
            },
            IssueWizardStep::Preview => self.handle_preview(*code),
            IssueWizardStep::Failed => {
                if *code == KeyCode::Esc {
                    // Back to the gate: a retry is armed ⏎ ⏎ like the first
                    // try, never one key.
                    self.step = IssueWizardStep::Preview;
                    self.create_error = None;
                    self.create_gate.reset();
                    self.refresh_gate();
                }
            }
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
            IssueWizardStep::Dependencies => return self.handle_dependencies(event, *code),
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let now = self.now();
        self.create_gate.tick(now);
        self.fields.refresh_focus_styles();
        self.refresh_field_blocks();
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        // Form fields and the Blocked-by filter capture printable characters
        // as input — request Insert mode so the global `q` shortcut doesn't
        // steal them. The Type row is a NORMAL stop.
        let typing = match self.step {
            IssueWizardStep::BasicInfo | IssueWizardStep::DorFields => !self.type_focused,
            IssueWizardStep::Dependencies => self.dep_filter_editing,
            _ => false,
        };
        Some(if typing {
            InputMode::Insert
        } else {
            InputMode::Normal
        })
    }

    fn footer_hints(&self) -> Vec<(&'static str, &'static str)> {
        draw::footer_hints(self)
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
    fn new_starts_on_basics_with_title_focused() {
        let s = IssueWizardScreen::new();
        assert_eq!(s.step(), IssueWizardStep::BasicInfo);
        assert_eq!(s.focused_field(), Some(FieldId::Title));
        assert_eq!(s.desired_input_mode(), Some(InputMode::Insert));
    }

    #[test]
    fn stepper_has_five_steps_and_create_states_sit_on_preview() {
        let labels = IssueWizardStep::STEPPER.map(|s| s.label());
        assert_eq!(
            labels,
            ["Basics", "Details", "Blocked by", "AI review", "Preview"]
        );
        assert_eq!(IssueWizardStep::BasicInfo.stepper_index(), 0);
        for state in [
            IssueWizardStep::Creating,
            IssueWizardStep::Complete,
            IssueWizardStep::Failed,
        ] {
            assert_eq!(state.stepper_index(), 4, "{state:?}");
        }
        assert_eq!(IssueWizardStep::Preview.next(), None);
        assert_eq!(IssueWizardStep::BasicInfo.previous(), None);
    }

    #[test]
    fn enter_on_empty_basics_stays_and_marks_fields_touched() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::BasicInfo);
        assert_eq!(s.focused_field(), Some(FieldId::Title));
        assert!(s.field_error(FieldId::Title).is_some());
        assert!(s.field_error(FieldId::Overview).is_some());
    }

    #[test]
    fn untouched_empty_fields_show_no_border_error() {
        let s = IssueWizardScreen::new();
        assert!(s.field_error(FieldId::Title).is_none());
        assert_eq!(s.validation_errors().len(), 4, "the checklist sees all");
    }

    #[test]
    fn esc_on_first_step_returns_pop() {
        let mut s = IssueWizardScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
        assert_eq!(action, ScreenAction::Pop);
    }

    // ---- Type row (Basics, between Title and Overview) ----

    #[test]
    fn type_row_default_is_feature() {
        let s = IssueWizardScreen::new();
        assert_eq!(s.payload().issue_type, IssueType::Feature);
    }

    #[test]
    fn tab_from_title_reaches_type_row_then_overview() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        assert!(s.type_focused);
        assert_eq!(s.desired_input_mode(), Some(InputMode::Normal));
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        assert!(!s.type_focused);
        assert_eq!(s.focused_field(), Some(FieldId::Overview));
        s.handle_input(&key_event(KeyCode::BackTab), InputMode::Insert);
        assert!(s.type_focused);
        s.handle_input(&key_event(KeyCode::BackTab), InputMode::Normal);
        assert_eq!(s.focused_field(), Some(FieldId::Title));
    }

    #[test]
    fn type_row_arrows_and_f_b_pick_feature_or_bug() {
        let mut s = IssueWizardScreen::new();
        s.focus_type_row();
        s.handle_input(&key_event(KeyCode::Right), InputMode::Normal);
        assert_eq!(s.payload().issue_type, IssueType::Bug);
        s.handle_input(&key_event(KeyCode::Left), InputMode::Normal);
        assert_eq!(s.payload().issue_type, IssueType::Feature);
        s.handle_input(&key_event(KeyCode::Char('b')), InputMode::Normal);
        assert_eq!(s.payload().issue_type, IssueType::Bug);
        s.handle_input(&key_event(KeyCode::Char('f')), InputMode::Normal);
        assert_eq!(s.payload().issue_type, IssueType::Feature);
        assert_eq!(s.field_text(FieldId::Title), "", "letters never type");
    }

    // ---- BasicInfo ----

    fn at_basic_info() -> IssueWizardScreen {
        IssueWizardScreen::new()
    }

    /// Title → Type row → Overview.
    fn tab_to_overview(s: &mut IssueWizardScreen) {
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
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
        tab_to_overview(&mut s);
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
        tab_to_overview(&mut s);
        type_string(&mut s, "overview");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::DorFields);
    }

    #[test]
    fn basic_info_shift_enter_inserts_newline_in_overview() {
        let mut s = at_basic_info();
        tab_to_overview(&mut s);
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
        tab_to_overview(&mut s);
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
    fn dor_fields_advance_blocked_when_expected_behavior_empty() {
        let mut s = at_dor_fields(IssueType::Feature);
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        type_string(&mut s, "criteria");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::DorFields);
        assert_eq!(
            s.focused_field(),
            Some(FieldId::ExpectedBehavior),
            "Enter focuses the first invalid field"
        );
        assert!(s.field_error(FieldId::ExpectedBehavior).is_some());
    }

    #[test]
    fn dor_fields_advance_succeeds_with_expected_and_acceptance_filled() {
        let mut s = at_dor_fields(IssueType::Feature);
        type_string(&mut s, "expected");
        // Move to acceptance criteria field
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        assert_eq!(s.focused_field(), Some(FieldId::AcceptanceCriteria));
        type_string(&mut s, "criteria");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::Dependencies);
    }

    #[test]
    fn esc_in_details_returns_to_basics() {
        let mut s = at_dor_fields(IssueType::Feature);
        s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::BasicInfo);
    }

    // ---- Insert input mode for capturing chars ----

    #[test]
    fn basic_info_desires_insert_mode() {
        let s = at_basic_info();
        assert_eq!(s.desired_input_mode(), Some(InputMode::Insert));
    }

    #[test]
    fn type_row_desires_normal_mode() {
        let mut s = IssueWizardScreen::new();
        s.focus_type_row();
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
        IssueWizardScreen::with_clipboard(Box::new(FakeClipboard::new(clip)))
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
        tab_to_overview(&mut s);
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
        let p = IssueCreationPayload {
            title: "t".into(),
            overview: "o".into(),
            expected_behavior: "e".into(),
            acceptance_criteria: "a".into(),
            image_paths: vec!["/tmp/a.png".into(), "/tmp/b.png".into()],
            ..Default::default()
        };
        let body = render_body_markdown(&p);
        assert!(body.contains("## Attachments"));
        assert!(body.contains("[Attached image: /tmp/a.png]"));
        assert!(body.contains("[Attached image: /tmp/b.png]"));
    }

    #[test]
    fn render_body_omits_attachments_section_when_empty() {
        let p = IssueCreationPayload {
            title: "t".into(),
            overview: "o".into(),
            expected_behavior: "e".into(),
            acceptance_criteria: "a".into(),
            ..Default::default()
        };
        let body = render_body_markdown(&p);
        assert!(!body.contains("## Attachments"));
    }

    // ---- #295 Dependencies step ----

    fn make_open_issue(number: u64) -> crate::provider::types::Issue {
        crate::provider::types::Issue {
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
        type_string(&mut s, "expected");
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
        // Jump directly for testing (no sync: the Basics textareas are empty).
        s.set_step_for_tests(IssueWizardStep::Preview);
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
    fn enter_enter_on_preview_begins_create() {
        let mut s = at_preview();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert!(!s.create_in_flight(), "one Enter only arms");
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
        assert_eq!(s.step(), IssueWizardStep::BasicInfo);
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
    fn esc_on_failed_retries_create_through_the_armed_gate() {
        let mut s = at_preview();
        s.begin_create();
        s.finish_create(Err(anyhow::anyhow!("boom")));
        s.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
        assert_eq!(s.step(), IssueWizardStep::Failed, "r is unbound on Failed");
        s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(s.step(), IssueWizardStep::Preview);
        assert!(!s.create_in_flight(), "Esc alone never writes");
        assert!(s.create_error().is_none());
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(s.step(), IssueWizardStep::Creating);
        assert!(s.create_in_flight());
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

    // ---- #450 AI Improve companion ----

    fn at_ai_review_with_critique() -> IssueWizardScreen {
        let mut s = IssueWizardScreen::new();
        s.payload_mut().issue_type = IssueType::Feature;
        s.payload_mut().title = "Original title".into();
        s.payload_mut().overview = "original".into();
        s.payload_mut().expected_behavior = "orig".into();
        s.payload_mut().acceptance_criteria = "orig ac".into();
        s.payload_mut().files_to_modify = "orig".into();
        s.payload_mut().test_hints = "orig".into();
        s.payload_mut().blocked_by = vec![10];
        s.payload_mut().milestone = Some(42);
        s.set_step_for_tests(IssueWizardStep::AiReview);
        s.begin_ai_review();
        s.apply_ai_review(Ok("critique text".into()));
        s
    }

    fn sample_improved() -> IssueCreationPayload {
        IssueCreationPayload {
            issue_type: IssueType::Feature,
            title: "Improved title".into(),
            overview: "improved".into(),
            expected_behavior: "improved".into(),
            current_behavior: String::new(),
            steps_to_reproduce: String::new(),
            acceptance_criteria: "improved ac".into(),
            files_to_modify: "improved".into(),
            test_hints: "improved".into(),
            blocked_by: vec![10],
            milestone: Some(42),
            image_paths: vec![],
        }
    }

    #[test]
    fn begin_improve_sets_loading_clears_candidate_and_error() {
        let mut s = at_ai_review_with_critique();
        s.begin_improve();
        assert!(s.improve_loading());
        assert!(s.improve_candidate().is_none());
        assert!(s.improve_error().is_none());
    }

    #[test]
    fn begin_improve_is_noop_while_review_still_loading() {
        let mut s = IssueWizardScreen::new();
        s.set_step_for_tests(IssueWizardStep::AiReview);
        s.begin_ai_review(); // review_loading=true, review_text=None
        s.begin_improve();
        assert!(
            !s.improve_loading(),
            "begin_improve should be a no-op while review is loading"
        );
    }

    #[test]
    fn begin_improve_is_noop_when_review_has_error() {
        let mut s = IssueWizardScreen::new();
        s.set_step_for_tests(IssueWizardStep::AiReview);
        s.begin_ai_review();
        s.apply_ai_review(Err("review failed".into()));
        s.begin_improve();
        assert!(!s.improve_loading());
    }

    #[test]
    fn begin_improve_is_noop_when_candidate_already_present() {
        let mut s = at_ai_review_with_critique();
        s.apply_improve_result(Ok(sample_improved()));
        assert!(s.improve_candidate().is_some());
        s.begin_improve();
        assert!(
            !s.improve_loading(),
            "begin_improve must not re-fire while a candidate is on screen"
        );
    }

    #[test]
    fn apply_improve_result_ok_populates_candidate() {
        let mut s = at_ai_review_with_critique();
        s.begin_improve();
        s.apply_improve_result(Ok(sample_improved()));
        assert!(!s.improve_loading());
        assert!(s.improve_candidate().is_some());
        assert!(s.improve_error().is_none());
    }

    #[test]
    fn apply_improve_result_err_populates_error() {
        let mut s = at_ai_review_with_critique();
        s.begin_improve();
        s.apply_improve_result(Err("bad JSON".into()));
        assert!(!s.improve_loading());
        assert!(s.improve_candidate().is_none());
        assert_eq!(s.improve_error(), Some("bad JSON"));
    }

    #[test]
    fn accepting_every_section_replaces_payload_and_drops_candidate() {
        let mut s = at_ai_review_with_critique();
        s.apply_improve_result(Ok(sample_improved()));
        for _ in s.changed_sections() {
            s.handle_input(&key_event(KeyCode::Char('a')), InputMode::Normal);
            s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(s.payload().title, "Improved title");
        assert_eq!(s.payload().test_hints, "improved");
        assert!(s.improve_candidate().is_none());
        assert_eq!(
            s.review_text(),
            Some("critique text"),
            "critique stays visible after accept"
        );
    }

    #[test]
    fn discard_improve_drops_candidate_leaves_payload_intact() {
        let mut s = at_ai_review_with_critique();
        let original_title = s.payload().title.clone();
        s.apply_improve_result(Ok(sample_improved()));
        s.discard_improve();
        assert_eq!(s.payload().title, original_title);
        assert!(s.improve_candidate().is_none());
        assert!(s.improve_error().is_none());
    }

    #[test]
    fn improve_requested_returns_true_exactly_once_per_begin() {
        let mut s = at_ai_review_with_critique();
        s.begin_improve();
        assert!(s.improve_requested());
        s.mark_improve_enqueued();
        assert!(
            !s.improve_requested(),
            "after marking enqueued, the tick hook must not re-dispatch"
        );
    }

    #[test]
    fn i_key_calls_begin_improve_when_guard_passes() {
        let mut s = at_ai_review_with_critique();
        s.handle_input(&key_event(KeyCode::Char('i')), InputMode::Normal);
        assert!(s.improve_loading());
    }

    #[test]
    fn i_key_is_ignored_while_review_loading() {
        let mut s = IssueWizardScreen::new();
        s.set_step_for_tests(IssueWizardStep::AiReview);
        s.begin_ai_review(); // review_loading=true
        s.handle_input(&key_event(KeyCode::Char('i')), InputMode::Normal);
        assert!(!s.improve_loading());
    }

    #[test]
    fn a_and_d_keys_are_silently_ignored_without_candidate() {
        let mut s = at_ai_review_with_critique();
        let original_title = s.payload().title.clone();
        s.handle_input(&key_event(KeyCode::Char('a')), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        assert_eq!(s.payload().title, original_title);
        assert!(s.improve_candidate().is_none());
    }

    #[test]
    fn a_key_accepts_the_focused_section_only() {
        let mut s = at_ai_review_with_critique();
        s.apply_improve_result(Ok(sample_improved()));
        s.handle_input(&key_event(KeyCode::Char('a')), InputMode::Normal);
        assert_eq!(s.decisions()[0], Decision::Accepted, "Title is first");
        assert!(s.improve_candidate().is_some(), "a alone merges nothing");
        assert_eq!(s.payload().title, "Original title");
    }

    #[test]
    fn d_key_keeps_mine_for_the_focused_section() {
        let mut s = at_ai_review_with_critique();
        let original_title = s.payload().title.clone();
        s.apply_improve_result(Ok(sample_improved()));
        s.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        assert_eq!(s.decisions()[0], Decision::KeptMine);
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(s.payload().title, original_title);
        assert!(s.improve_candidate().is_none());
    }

    #[test]
    fn esc_on_the_diff_discards_the_candidate() {
        let mut s = at_ai_review_with_critique();
        let original = s.payload().overview.clone();
        s.apply_improve_result(Ok(sample_improved()));
        s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert!(s.improve_candidate().is_none());
        assert_eq!(s.payload().overview, original);
        assert_eq!(s.step(), IssueWizardStep::AiReview);
    }

    #[test]
    fn esc_with_improve_error_clears_error() {
        let mut s = at_ai_review_with_critique();
        s.begin_improve();
        s.apply_improve_result(Err("boom".into()));
        assert!(s.improve_error().is_some());
        s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert!(s.improve_error().is_none());
    }

    // ---- #455 already-exists modal ----

    #[test]
    fn finish_create_already_exists_sets_modal() {
        let mut s = IssueWizardScreen::new();
        s.finish_create_already_exists(42, "open".into(), "feat: login".into());
        let Some(modal) = s.already_exists_modal() else {
            panic!("modal must be set");
        };
        assert_eq!(modal.number, 42);
        assert_eq!(modal.state, "open");
        assert_eq!(modal.title, "feat: login");
        assert_eq!(modal.focus, 0);
    }

    #[test]
    fn already_exists_modal_edit_returns_to_basic_info() {
        let mut s = IssueWizardScreen::new();
        // Move to Creating step first to simulate mid-flight failure.
        s.jump_to(IssueWizardStep::Creating);
        s.finish_create_already_exists(42, "open".into(), "feat: login".into());
        // Enter on focus=0 (Edit) should dismiss modal + go to BasicInfo.
        let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert!(s.already_exists_modal().is_none());
        assert_eq!(s.step(), IssueWizardStep::BasicInfo);
    }

    #[test]
    fn already_exists_modal_cancel_pops_wizard() {
        let mut s = IssueWizardScreen::new();
        s.finish_create_already_exists(42, "open".into(), "feat: login".into());
        // Toggle focus to Cancel (focus=1).
        s.handle_input(&key_event(KeyCode::Right), InputMode::Normal);
        let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
        assert!(s.already_exists_modal().is_none());
    }

    #[test]
    fn already_exists_modal_esc_cancels_and_pops() {
        let mut s = IssueWizardScreen::new();
        s.finish_create_already_exists(42, "open".into(), "feat: login".into());
        let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
        assert!(s.already_exists_modal().is_none());
    }

    #[test]
    fn already_exists_modal_swallows_other_keys() {
        let mut s = IssueWizardScreen::new();
        s.finish_create_already_exists(42, "open".into(), "feat: login".into());
        // Modal swallows — payload title should not accumulate random char input.
        let before = s.payload().title.clone();
        s.handle_input(&key_event(KeyCode::Char('x')), InputMode::Normal);
        assert_eq!(s.payload().title, before);
        // Modal is still present.
        assert!(s.already_exists_modal().is_some());
    }

    #[test]
    fn already_exists_modal_toggle_focus() {
        let mut s = IssueWizardScreen::new();
        s.finish_create_already_exists(42, "open".into(), "feat: login".into());
        assert_eq!(s.already_exists_modal().unwrap().focus, 0);
        s.already_exists_modal_toggle_focus();
        assert_eq!(s.already_exists_modal().unwrap().focus, 1);
        s.already_exists_modal_toggle_focus();
        assert_eq!(s.already_exists_modal().unwrap().focus, 0);
    }

    #[test]
    fn basic_info_validation_rejects_whitespace_only_title() {
        let mut s = IssueWizardScreen::new();
        s.jump_to(IssueWizardStep::BasicInfo);
        // Type whitespace only
        s.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Insert);
        s.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Insert);
        assert!(s.step_errors().iter().any(|(f, _)| *f == FieldId::Title));
    }

    #[test]
    fn basic_info_validation_rejects_leading_dash_title() {
        let mut s = IssueWizardScreen::new();
        s.jump_to(IssueWizardStep::BasicInfo);
        for c in "-evil".chars() {
            s.handle_input(&key_event(KeyCode::Char(c)), InputMode::Insert);
        }
        assert!(s.step_errors().iter().any(|(f, _)| *f == FieldId::Title));
    }
}

#[cfg(test)]
#[path = "flow_tests.rs"]
mod flow_tests;

#[cfg(test)]
#[path = "flow_guard_tests.rs"]
mod flow_guard_tests;
