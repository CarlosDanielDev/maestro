pub mod ai_planning;
mod draw;
pub mod types;

pub use ai_planning::{build_planning_prompt, parse_planning_response};
pub use types::{AiGeneratedPlan, AiProposedIssue, MilestonePlanPayload, MilestoneWizardStep};

/// Result of a successful milestone+issues materialization (#297, updated #455).
///
/// Distinct variants for freshly-created versus reused milestones so the
/// wizard's completion screen can report honestly about what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MilestoneCreationResult {
    /// Milestone was newly created; `issue_numbers` are all freshly created.
    Created {
        milestone_number: u64,
        issue_numbers: Vec<u64>,
    },
    /// Milestone matched an existing one. `new_issue_numbers` are the
    /// issues that were newly created under the reused milestone;
    /// `skipped_issue_numbers` are pre-existing issues that matched by
    /// title.
    Reused {
        milestone_number: u64,
        new_issue_numbers: Vec<u64>,
        skipped_issue_numbers: Vec<u64>,
    },
}

impl MilestoneCreationResult {
    #[allow(dead_code)]
    pub fn milestone_number(&self) -> u64 {
        match self {
            Self::Created {
                milestone_number, ..
            } => *milestone_number,
            Self::Reused {
                milestone_number, ..
            } => *milestone_number,
        }
    }

    /// All issue numbers visible under the milestone — useful for callers
    /// that just want to know "what ended up there".
    #[allow(dead_code)]
    pub fn all_issue_numbers(&self) -> Vec<u64> {
        match self {
            Self::Created { issue_numbers, .. } => issue_numbers.clone(),
            Self::Reused {
                new_issue_numbers,
                skipped_issue_numbers,
                ..
            } => {
                let mut out = new_issue_numbers.clone();
                out.extend(skipped_issue_numbers.iter().copied());
                out
            }
        }
    }
}

/// Group accepted issues by their dependency level (length of the longest
/// `blocked_by` chain). Level 0 has no dependencies; Level N depends on
/// at least one issue at Level N-1. Used by Preview rendering and the
/// dependency graph generator.
pub fn level_buckets(issues: &[AiProposedIssue]) -> Vec<Vec<usize>> {
    let mut levels: Vec<Option<usize>> = vec![None; issues.len()];
    let mut buckets: Vec<Vec<usize>> = Vec::new();
    let mut progress = true;
    while progress {
        progress = false;
        for (i, issue) in issues.iter().enumerate() {
            if levels[i].is_some() || !issue.accepted {
                continue;
            }
            let dep_levels: Option<Vec<usize>> = issue
                .blocked_by
                .iter()
                .map(|d| issues.get(*d).and(levels[*d]))
                .collect();
            if let Some(deps) = dep_levels {
                let level = deps.into_iter().max().map_or(0, |m| m + 1);
                levels[i] = Some(level);
                while buckets.len() <= level {
                    buckets.push(Vec::new());
                }
                buckets[level].push(i);
                progress = true;
            }
        }
    }
    buckets
}

/// Materialize an accepted plan into GitHub: create the milestone, then
/// each accepted issue in dependency order with its `Blocked By` section
/// rewritten to use actual issue numbers. Used by the `Materializing`
/// step's background task (#297, duplicate-aware via #455).
pub async fn materialize_plan(
    plan: &AiGeneratedPlan,
    repo: Option<String>,
) -> Result<MilestoneCreationResult, String> {
    use crate::provider::github::client::GhCliClient;
    let client = GhCliClient::from_config_repo(repo);
    materialize_plan_with_client(plan, &client).await
}

/// Inner impl parameterized over any `GitHubClient`. Exposed so tests
/// can drive a mock through the same logic as production.
pub async fn materialize_plan_with_client<G: crate::provider::github::client::GitHubClient>(
    plan: &AiGeneratedPlan,
    client: &G,
) -> Result<MilestoneCreationResult, String> {
    use crate::provider::github::client::CreateOutcome;

    let ms_outcome = client
        .create_milestone(&plan.milestone_title, &plan.milestone_description)
        .await
        .map_err(|e| format!("create_milestone failed: {e}"))?;
    let milestone_number = ms_outcome.number();
    let milestone_reused = ms_outcome.is_existed();

    let buckets = level_buckets(&plan.issues);
    let mut number_for_index: std::collections::HashMap<usize, u64> =
        std::collections::HashMap::new();
    let mut new_numbers: Vec<u64> = Vec::new();
    let mut skipped_numbers: Vec<u64> = Vec::new();

    for level in buckets {
        for idx in level {
            let issue = &plan.issues[idx];
            let blocked_lines: Vec<String> = issue
                .blocked_by
                .iter()
                .filter_map(|d| number_for_index.get(d).map(|n| format!("- #{}", n)))
                .collect();
            let blocked_by_section = if blocked_lines.is_empty() {
                "## Blocked By\n\n- None\n".to_string()
            } else {
                format!("## Blocked By\n\n{}\n", blocked_lines.join("\n"))
            };
            let body = format!(
                "## Overview\n\n{}\n\n{}",
                issue.overview.trim(),
                blocked_by_section
            );
            let labels = vec!["enhancement".to_string(), "maestro:ready".to_string()];
            let outcome = client
                .create_issue(&issue.title, &body, &labels, Some(milestone_number))
                .await
                .map_err(|e| format!("create_issue '{}' failed: {e}", issue.title))?;
            let num = outcome.number();
            number_for_index.insert(idx, num);
            match outcome {
                CreateOutcome::Created(_) => new_numbers.push(num),
                CreateOutcome::Existed { .. } => skipped_numbers.push(num),
            }
        }
    }

    if milestone_reused {
        Ok(MilestoneCreationResult::Reused {
            milestone_number,
            new_issue_numbers: new_numbers,
            skipped_issue_numbers: skipped_numbers,
        })
    } else {
        // Milestone freshly created. Put all issues (including any skipped
        // dupes from other milestones) into issue_numbers for simplicity.
        let mut all = new_numbers;
        all.extend(skipped_numbers);
        Ok(MilestoneCreationResult::Created {
            milestone_number,
            issue_numbers: all,
        })
    }
}

/// Build the `Sequence:` line shown at the bottom of the Preview step.
/// Mirrors the project convention: `#a → #b ∥ #c → #d`.
pub fn sequence_line(issues: &[AiProposedIssue]) -> String {
    let levels = level_buckets(issues);
    let parts: Vec<String> = levels
        .iter()
        .filter(|l| !l.is_empty())
        .map(|level| {
            let titles: Vec<String> = level
                .iter()
                .map(|&idx| {
                    let title = &issues[idx].title;
                    if title.len() > 24 {
                        format!("{}…", &title[..23])
                    } else {
                        title.clone()
                    }
                })
                .collect();
            titles.join(" ∥ ")
        })
        .collect();
    if parts.is_empty() {
        "Sequence: (empty)".to_string()
    } else {
        format!("Sequence: {}", parts.join(" → "))
    }
}

use super::prompt_input::{ClipboardContent, ClipboardProvider, SystemClipboard};
use super::wizard_fields::TextAreaField;
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

/// AI-guided wizard for milestone planning. #294 set up the scaffold
/// and AI launch; #297 wires the Review/Preview/Materializing/Complete
/// steps and the GitHub creation chain.
pub struct MilestoneWizardScreen {
    step: MilestoneWizardStep,
    payload: MilestonePlanPayload,
    pub(super) goal_field: TextAreaField,
    pub(super) non_goals_field: TextAreaField,
    pub(super) doc_buffer_field: TextAreaField,
    clipboard: Box<dyn ClipboardProvider>,
    planning_in_flight: bool,
    generated_plan: Option<AiGeneratedPlan>,
    failure_reason: Option<String>,
    /// Index focused on the Review step.
    pub(super) review_focus: usize,
    /// Materialization progress: (created, total) when a creation is in flight.
    pub(super) materialize_progress: Option<(usize, usize)>,
    /// True between `begin_materialization()` and the moment dispatch
    /// enqueues the `CreateMilestoneWithIssues` command. Prevents
    /// duplicate dispatches while keeping `materialize_progress` live
    /// until the background task completes.
    pub(super) materialize_enqueued: bool,
    /// Numbers of GitHub issues that succeeded; populated as `MilestonePlanCreated` arrives.
    pub(super) created_issue_numbers: Vec<u64>,
    /// Created milestone number on success.
    pub(super) created_milestone_number: Option<u64>,
    /// Whether the milestone was reused (matched an existing one) instead
    /// of freshly created.
    pub(super) milestone_reused: bool,
    /// Numbers of pre-existing issues that were skipped during materialization.
    pub(super) skipped_issue_numbers: Vec<u64>,
}

impl MilestoneWizardScreen {
    pub fn new() -> Self {
        Self::with_clipboard(Box::new(SystemClipboard))
    }

    pub fn with_clipboard(clipboard: Box<dyn ClipboardProvider>) -> Self {
        Self {
            step: MilestoneWizardStep::default(),
            payload: MilestonePlanPayload::default(),
            goal_field: TextAreaField::multi_line(),
            non_goals_field: TextAreaField::multi_line(),
            doc_buffer_field: TextAreaField::single_line(),
            clipboard,
            planning_in_flight: false,
            generated_plan: None,
            failure_reason: None,
            review_focus: 0,
            materialize_progress: None,
            materialize_enqueued: false,
            created_issue_numbers: Vec::new(),
            created_milestone_number: None,
            milestone_reused: false,
            skipped_issue_numbers: Vec::new(),
        }
    }

    /// Route a pasted payload into the active text surface for the
    /// current step. `TextAreaField::insert_sanitized` handles the
    /// control-char filter; on `DocReferences`, newline-separated
    /// lines each commit as a separate reference (matches how users
    /// typically copy path lists).
    pub fn paste_text_into_active(&mut self, text: &str) {
        match self.step {
            MilestoneWizardStep::GoalDefinition => {
                self.goal_field.insert_sanitized(text);
            }
            MilestoneWizardStep::NonGoals => {
                self.non_goals_field.insert_sanitized(text);
            }
            MilestoneWizardStep::DocReferences => {
                // Filter control chars once so the branch logic below
                // can split on `\n` cleanly (stray `\x1b` etc. won't
                // mask a newline).
                let sanitised: String = text
                    .chars()
                    .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
                    .collect();
                if sanitised.contains('\n') {
                    for line in sanitised
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty())
                    {
                        self.doc_buffer_field.set_text(line);
                        self.commit_doc_buffer();
                    }
                } else {
                    self.doc_buffer_field.insert_sanitized(&sanitised);
                }
            }
            _ => {}
        }
        self.sync_fields_into_payload();
    }

    /// Ctrl+V handler — image clipboard content is captured as an
    /// attachment, text is routed through `paste_text_into_active`.
    pub fn paste_from_clipboard(&mut self) {
        match self.clipboard.read() {
            ClipboardContent::Image(path) => {
                self.payload
                    .image_paths
                    .push(path.to_string_lossy().to_string());
            }
            ClipboardContent::Text(text) => {
                self.paste_text_into_active(&text);
            }
            ClipboardContent::Empty | ClipboardContent::Unavailable => {}
        }
    }

    /// Flush live textarea content into `payload.goals` and
    /// `payload.non_goals`. `payload.doc_references` is a commit-
    /// per-entry list and is not covered here — entries land there
    /// via `commit_doc_buffer`.
    fn sync_fields_into_payload(&mut self) {
        self.payload.goals = self.goal_field.text();
        self.payload.non_goals = self.non_goals_field.text();
    }

    /// Re-seed the Goals / Non-Goals textareas from `payload` after
    /// an external caller has mutated `payload_mut()`. Not invoked by
    /// the wizard itself — it exists so tests can set payload directly
    /// and still have the textareas render the same content.
    #[cfg(test)]
    pub(crate) fn reseed_fields_from_payload(&mut self) {
        self.goal_field.set_text(&self.payload.goals);
        self.non_goals_field.set_text(&self.payload.non_goals);
    }

    pub fn materialize_enqueued(&self) -> bool {
        self.materialize_enqueued
    }

    pub fn mark_materialize_enqueued(&mut self) {
        self.materialize_enqueued = true;
    }

    pub fn review_focus(&self) -> usize {
        self.review_focus
    }

    pub fn materialize_progress(&self) -> Option<(usize, usize)> {
        self.materialize_progress
    }

    pub fn created_milestone_number(&self) -> Option<u64> {
        self.created_milestone_number
    }

    pub fn created_issue_numbers(&self) -> &[u64] {
        &self.created_issue_numbers
    }

    pub fn milestone_was_reused(&self) -> bool {
        self.milestone_reused
    }

    pub fn skipped_issue_numbers(&self) -> &[u64] {
        &self.skipped_issue_numbers
    }

    /// #297 Materialization lifecycle hooks.
    pub fn begin_materialization(&mut self) {
        // #455 — refuse to dispatch if the AI-generated plan contains
        // invalid titles (empty, > 256 bytes, leading `-`, null byte).
        // Failing loudly here is better than letting the creation call
        // error out with a 422 halfway through the batch.
        if let Some(err) = self.plan_validation_error() {
            self.failure_reason = Some(err);
            self.step = MilestoneWizardStep::Failed;
            return;
        }

        let total = self
            .generated_plan
            .as_ref()
            .map(|p| p.issues.iter().filter(|i| i.accepted).count())
            .unwrap_or(0);
        self.materialize_progress = Some((0, total));
        self.materialize_enqueued = false;
        self.created_issue_numbers.clear();
        self.created_milestone_number = None;
        self.milestone_reused = false;
        self.skipped_issue_numbers.clear();
        self.failure_reason = None;
        self.step = MilestoneWizardStep::Materializing;
    }

    /// Validate every title in the generated plan against the canonical
    /// #452 helpers. Returns `Some(reason)` on the first failure.
    pub fn plan_validation_error(&self) -> Option<String> {
        let plan = self.generated_plan.as_ref()?;
        if let Err(e) = crate::util::validate_title(&plan.milestone_title, "milestone title") {
            return Some(format!("Invalid milestone title: {e}"));
        }
        for issue in plan.issues.iter().filter(|i| i.accepted) {
            if let Err(e) = crate::util::validate_title(&issue.title, "issue title") {
                return Some(format!("Invalid issue title '{}': {e}", issue.title));
            }
        }
        None
    }

    pub fn finish_materialization(&mut self, result: Result<MilestoneCreationResult, String>) {
        self.materialize_progress = None;
        self.materialize_enqueued = false;
        match result {
            Ok(MilestoneCreationResult::Created {
                milestone_number,
                issue_numbers,
            }) => {
                self.created_milestone_number = Some(milestone_number);
                self.created_issue_numbers = issue_numbers;
                self.milestone_reused = false;
                self.skipped_issue_numbers = Vec::new();
                self.step = MilestoneWizardStep::Complete;
            }
            Ok(MilestoneCreationResult::Reused {
                milestone_number,
                new_issue_numbers,
                skipped_issue_numbers,
            }) => {
                self.created_milestone_number = Some(milestone_number);
                self.created_issue_numbers = new_issue_numbers;
                self.milestone_reused = true;
                self.skipped_issue_numbers = skipped_issue_numbers;
                self.step = MilestoneWizardStep::Complete;
            }
            Err(e) => {
                self.failure_reason = Some(e);
                self.step = MilestoneWizardStep::Failed;
            }
        }
    }

    fn review_focus_inc(&mut self) {
        let total = self
            .generated_plan
            .as_ref()
            .map(|p| p.issues.len())
            .unwrap_or(0);
        if total > 0 && self.review_focus + 1 < total {
            self.review_focus += 1;
        }
    }

    fn review_focus_dec(&mut self) {
        self.review_focus = self.review_focus.saturating_sub(1);
    }

    fn review_toggle_focused(&mut self, accepted: bool) {
        if let Some(plan) = self.generated_plan.as_mut()
            && let Some(issue) = plan.issues.get_mut(self.review_focus)
        {
            issue.accepted = accepted;
        }
    }

    fn entered_ai_structuring(&self) -> bool {
        matches!(self.step, MilestoneWizardStep::AiStructuring)
            && !self.planning_in_flight
            && self.generated_plan.is_none()
            && self.failure_reason.is_none()
    }

    /// Whether the AI Structuring step's automatic launch should fire.
    pub fn entered_ai_structuring_step(&self) -> bool {
        self.entered_ai_structuring()
    }

    pub fn step(&self) -> MilestoneWizardStep {
        self.step
    }

    pub fn payload(&self) -> &MilestonePlanPayload {
        &self.payload
    }

    #[cfg(test)]
    pub fn doc_buffer(&self) -> String {
        self.doc_buffer_field.text()
    }

    pub fn is_planning_in_flight(&self) -> bool {
        self.planning_in_flight
    }

    pub fn has_generated_plan(&self) -> bool {
        self.generated_plan.is_some()
    }

    pub fn generated_plan(&self) -> Option<&AiGeneratedPlan> {
        self.generated_plan.as_ref()
    }

    pub fn failure_reason(&self) -> Option<&str> {
        self.failure_reason.as_deref()
    }

    /// Validate the current step is allowed to advance. Reads the live
    /// `goal_field` textarea so the footer error updates as the user
    /// types (no wait for a sync boundary).
    pub fn validation_error(&self) -> Option<&'static str> {
        match self.step {
            MilestoneWizardStep::GoalDefinition => {
                if self.goal_field.text().trim().is_empty() {
                    Some("Goals are required")
                } else {
                    None
                }
            }
            MilestoneWizardStep::AiStructuring if self.planning_in_flight => {
                Some("AI is still working — please wait")
            }
            _ => None,
        }
    }

    pub fn try_advance(&mut self) -> bool {
        self.sync_fields_into_payload();
        if self.validation_error().is_some() {
            return false;
        }
        if let Some(next) = self.step.next() {
            self.step = next;
            true
        } else {
            false
        }
    }

    pub fn retreat(&mut self) -> bool {
        self.sync_fields_into_payload();
        if let Some(prev) = self.step.previous() {
            self.step = prev;
            true
        } else {
            false
        }
    }

    /// Validate a doc reference: URL when it starts with `http://` or
    /// `https://`, otherwise treat as a file path and check existence.
    pub fn validate_reference(s: &str) -> bool {
        if s.starts_with("http://") || s.starts_with("https://") {
            return true;
        }
        std::path::Path::new(s).exists()
    }

    /// Add the current doc-buffer as a new reference (validated). Clears
    /// the buffer field.
    pub fn commit_doc_buffer(&mut self) {
        let entry = self.doc_buffer_field.text().trim().to_string();
        if entry.is_empty() {
            return;
        }
        let valid = Self::validate_reference(&entry);
        self.payload.doc_references.push(entry);
        self.payload.doc_reference_valid.push(valid);
        self.doc_buffer_field.set_text("");
    }

    /// Begin an AI planning request. Caller (event loop) is responsible
    /// for actually spawning the work via `TuiCommand::LaunchAiPlanning`.
    pub fn start_planning(&mut self) {
        self.sync_fields_into_payload();
        self.planning_in_flight = true;
        self.generated_plan = None;
        self.failure_reason = None;
    }

    /// Apply a planning result. Clears the in-flight flag.
    pub fn apply_planning_result(&mut self, result: Result<AiGeneratedPlan, String>) {
        self.planning_in_flight = false;
        match result {
            Ok(plan) => {
                self.generated_plan = Some(plan);
                self.failure_reason = None;
            }
            Err(e) => {
                self.failure_reason = Some(e);
                self.step = MilestoneWizardStep::Failed;
            }
        }
    }

    /// Mutable reference to the textarea active on the current step,
    /// if any. Used to delegate raw key events (char insertion,
    /// backspace, arrows, selection, undo/redo) to tui-textarea.
    fn active_field_mut(&mut self) -> Option<&mut TextAreaField> {
        match self.step {
            MilestoneWizardStep::GoalDefinition => Some(&mut self.goal_field),
            MilestoneWizardStep::NonGoals => Some(&mut self.non_goals_field),
            MilestoneWizardStep::DocReferences => Some(&mut self.doc_buffer_field),
            _ => None,
        }
    }

    /// Refresh the border style on the active textarea to match focus
    /// visuals (focused = LightCyan + BOLD). Called from `Screen::draw`
    /// before rendering so `draw_impl` can stay `&self`.
    pub(super) fn refresh_field_blocks(&mut self) {
        let border_style = Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD);
        let block = |title: &'static str| {
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title)
        };
        match self.step {
            MilestoneWizardStep::GoalDefinition => {
                self.goal_field.area_mut().set_block(block("Your goals"));
            }
            MilestoneWizardStep::NonGoals => {
                self.non_goals_field
                    .area_mut()
                    .set_block(block("Non-goals"));
            }
            _ => {}
        }
        // Always-reversed cursor on the active field so it blinks
        // visibly; other fields keep their default (unused) style.
        self.goal_field
            .area_mut()
            .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        self.non_goals_field
            .area_mut()
            .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        self.doc_buffer_field
            .area_mut()
            .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
    }
}

impl Default for MilestoneWizardScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapProvider for MilestoneWizardScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Milestone Wizard",
            bindings: vec![
                KeyBinding {
                    key: "Enter",
                    description: "Next step / commit doc reference",
                },
                KeyBinding {
                    key: "Shift+Enter",
                    description: "Newline in goal/non-goal fields, or advance from Doc References",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Previous step (or close)",
                },
            ],
        }]
    }
}

impl Screen for MilestoneWizardScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Paste(text) = event {
            self.paste_text_into_active(text);
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

        if modifiers.contains(KeyModifiers::CONTROL) && *code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return ScreenAction::None;
        }

        match (self.step, code, *modifiers) {
            (_, KeyCode::Esc, _) => {
                if self.step.is_first() {
                    return ScreenAction::Pop;
                }
                self.retreat();
            }
            (MilestoneWizardStep::ReviewPlan, KeyCode::Char('j'), _)
            | (MilestoneWizardStep::ReviewPlan, KeyCode::Down, _) => {
                self.review_focus_inc();
            }
            (MilestoneWizardStep::ReviewPlan, KeyCode::Char('k'), _)
            | (MilestoneWizardStep::ReviewPlan, KeyCode::Up, _) => {
                self.review_focus_dec();
            }
            (MilestoneWizardStep::ReviewPlan, KeyCode::Char('a'), _) => {
                self.review_toggle_focused(true);
            }
            (MilestoneWizardStep::ReviewPlan, KeyCode::Char('x'), _) => {
                self.review_toggle_focused(false);
            }
            (MilestoneWizardStep::Preview, KeyCode::Enter, _) => {
                self.begin_materialization();
                return ScreenAction::None;
            }
            (MilestoneWizardStep::Failed, KeyCode::Char('r'), _) => {
                self.failure_reason = None;
                self.step = MilestoneWizardStep::Preview;
            }
            (MilestoneWizardStep::Complete, KeyCode::Enter, _) => {
                return ScreenAction::Pop;
            }
            (MilestoneWizardStep::DocReferences, KeyCode::Enter, m)
                if m.contains(KeyModifiers::SHIFT) =>
            {
                self.try_advance();
            }
            (MilestoneWizardStep::DocReferences, KeyCode::Enter, _) => {
                self.commit_doc_buffer();
            }
            (
                MilestoneWizardStep::GoalDefinition | MilestoneWizardStep::NonGoals,
                KeyCode::Enter,
                m,
            ) if m.contains(KeyModifiers::SHIFT) => {
                if let Some(field) = self.active_field_mut() {
                    field.area_mut().insert_newline();
                }
            }
            (_, KeyCode::Enter, _) => {
                self.try_advance();
            }
            (
                MilestoneWizardStep::GoalDefinition
                | MilestoneWizardStep::NonGoals
                | MilestoneWizardStep::DocReferences,
                _,
                _,
            ) => {
                // Delegate Char, Backspace, Arrows, Home/End, word-wise
                // jumps, Ctrl+W / Ctrl+Z / Ctrl+Y, selection (Shift+arrows),
                // etc. to the active TextArea.
                if let Some(field) = self.active_field_mut() {
                    field.area_mut().input(event.clone());
                }
            }
            _ => {}
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.refresh_field_blocks();
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        if matches!(
            self.step,
            MilestoneWizardStep::GoalDefinition
                | MilestoneWizardStep::NonGoals
                | MilestoneWizardStep::DocReferences
        ) {
            Some(InputMode::Insert)
        } else {
            Some(InputMode::Normal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};

    fn type_chars(s: &mut MilestoneWizardScreen, text: &str) {
        for c in text.chars() {
            s.handle_input(&key_event(KeyCode::Char(c)), InputMode::Insert);
        }
    }

    #[test]
    fn new_starts_at_goal_definition() {
        let s = MilestoneWizardScreen::new();
        assert_eq!(s.step(), MilestoneWizardStep::GoalDefinition);
    }

    #[test]
    fn step_total_is_nine() {
        assert_eq!(MilestoneWizardStep::total(), 9);
    }

    #[test]
    fn step_index_is_one_based() {
        assert_eq!(MilestoneWizardStep::GoalDefinition.index(), 1);
        assert_eq!(MilestoneWizardStep::Failed.index(), 9);
    }

    #[test]
    fn esc_on_first_step_pops() {
        let mut s = MilestoneWizardScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn goal_chars_appear_in_live_textarea() {
        let mut s = MilestoneWizardScreen::new();
        type_chars(&mut s, "ship");
        // Live textarea reflects the typed content immediately.
        assert_eq!(s.goal_field.text(), "ship");
        // Payload is only updated at step-transition / paste boundaries;
        // typing alone does not flush.
        assert!(s.payload().goals.is_empty());
    }

    #[test]
    fn goal_advance_blocked_when_empty() {
        let mut s = MilestoneWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), MilestoneWizardStep::GoalDefinition);
    }

    #[test]
    fn goal_advance_succeeds_when_filled() {
        let mut s = MilestoneWizardScreen::new();
        type_chars(&mut s, "x");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), MilestoneWizardStep::NonGoals);
    }

    #[test]
    fn goal_shift_enter_inserts_newline() {
        let mut s = MilestoneWizardScreen::new();
        type_chars(&mut s, "a");
        s.handle_input(
            &key_event_with_modifiers(KeyCode::Enter, KeyModifiers::SHIFT),
            InputMode::Insert,
        );
        type_chars(&mut s, "b");
        assert_eq!(s.goal_field.text(), "a\nb");
    }

    #[test]
    fn doc_refs_enter_commits_buffer() {
        let mut s = MilestoneWizardScreen::new();
        type_chars(&mut s, "x");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert); // → DocReferences
        type_chars(&mut s, "https://example.com");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.payload().doc_references.len(), 1);
        assert_eq!(s.payload().doc_references[0], "https://example.com");
        assert_eq!(s.payload().doc_reference_valid[0], true);
        assert_eq!(s.doc_buffer(), "");
    }

    #[test]
    fn doc_refs_shift_enter_advances() {
        let mut s = MilestoneWizardScreen::new();
        type_chars(&mut s, "x");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        s.handle_input(
            &key_event_with_modifiers(KeyCode::Enter, KeyModifiers::SHIFT),
            InputMode::Insert,
        );
        assert_eq!(s.step(), MilestoneWizardStep::AiStructuring);
    }

    #[test]
    fn validate_reference_accepts_url() {
        assert!(MilestoneWizardScreen::validate_reference(
            "https://example.com"
        ));
        assert!(MilestoneWizardScreen::validate_reference(
            "http://localhost"
        ));
    }

    #[test]
    fn validate_reference_rejects_missing_path() {
        assert!(!MilestoneWizardScreen::validate_reference(
            "/this/path/does/not/exist/in/this/repo"
        ));
    }

    #[test]
    fn validate_reference_accepts_existing_path() {
        // Cargo.toml exists at the project root in every test run.
        assert!(MilestoneWizardScreen::validate_reference("Cargo.toml"));
    }

    #[test]
    fn start_planning_sets_in_flight_flag() {
        let mut s = MilestoneWizardScreen::new();
        s.start_planning();
        assert!(s.is_planning_in_flight());
    }

    #[test]
    fn apply_planning_result_ok_clears_in_flight_and_stores_plan() {
        let mut s = MilestoneWizardScreen::new();
        s.start_planning();
        let plan = AiGeneratedPlan {
            milestone_title: "v0.20.0".into(),
            milestone_description: "desc".into(),
            issues: Vec::new(),
        };
        s.apply_planning_result(Ok(plan));
        assert!(!s.is_planning_in_flight());
        assert!(s.has_generated_plan());
        assert!(s.failure_reason().is_none());
    }

    #[test]
    fn apply_planning_result_err_transitions_to_failed_step() {
        let mut s = MilestoneWizardScreen::new();
        s.start_planning();
        s.apply_planning_result(Err("API down".into()));
        assert_eq!(s.step(), MilestoneWizardStep::Failed);
        assert_eq!(s.failure_reason(), Some("API down"));
    }

    // ---- Paste handling ----

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

    fn paste_event(text: &str) -> crossterm::event::Event {
        crossterm::event::Event::Paste(text.to_string())
    }

    #[test]
    fn bracketed_paste_on_goal_appends_to_goals() {
        let mut s = MilestoneWizardScreen::with_clipboard(Box::new(FakeClipboard::new(
            super::ClipboardContent::Empty,
        )));
        s.handle_input(&paste_event("first\nsecond"), InputMode::Insert);
        assert_eq!(s.payload().goals, "first\nsecond");
    }

    #[test]
    fn bracketed_paste_on_doc_refs_splits_lines_into_entries() {
        let mut s = MilestoneWizardScreen::with_clipboard(Box::new(FakeClipboard::new(
            super::ClipboardContent::Empty,
        )));
        type_chars(&mut s, "goal");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert); // → NonGoals
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert); // → DocReferences
        s.handle_input(
            &paste_event("https://example.com/a\nhttps://example.com/b"),
            InputMode::Insert,
        );
        assert_eq!(
            s.payload().doc_references,
            vec![
                "https://example.com/a".to_string(),
                "https://example.com/b".to_string(),
            ]
        );
    }

    #[test]
    fn ctrl_v_image_clipboard_attaches_to_payload() {
        let tmp = std::env::temp_dir().join("milestone-test.png");
        let mut s = MilestoneWizardScreen::with_clipboard(Box::new(FakeClipboard::new(
            super::ClipboardContent::Image(tmp.clone()),
        )));
        s.handle_input(
            &key_event_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL),
            InputMode::Insert,
        );
        assert_eq!(
            s.payload().image_paths,
            vec![tmp.to_string_lossy().to_string()]
        );
    }

    #[test]
    fn build_planning_prompt_includes_attachments_when_present() {
        let payload = MilestonePlanPayload {
            goals: "g".into(),
            image_paths: vec!["/tmp/a.png".into()],
            ..Default::default()
        };
        let prompt = ai_planning::build_planning_prompt(&payload);
        assert!(prompt.contains("## Attachments"));
        assert!(prompt.contains("[Attached image: /tmp/a.png]"));
    }

    #[test]
    fn ai_structuring_advance_blocked_while_in_flight() {
        let mut s = MilestoneWizardScreen::new();
        type_chars(&mut s, "x");
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert); // → NonGoals
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert); // → DocReferences
        s.handle_input(
            &key_event_with_modifiers(KeyCode::Enter, KeyModifiers::SHIFT),
            InputMode::Insert,
        ); // → AiStructuring
        assert_eq!(s.step(), MilestoneWizardStep::AiStructuring);
        s.start_planning();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), MilestoneWizardStep::AiStructuring);
    }

    // ---- #455 materialize_plan_with_client ----

    fn plan_with_one_issue() -> AiGeneratedPlan {
        AiGeneratedPlan {
            milestone_title: "M0: Foundation".into(),
            milestone_description: "Setup".into(),
            issues: vec![AiProposedIssue {
                title: "feat: login page".into(),
                overview: "Login page".into(),
                blocked_by: vec![],
                accepted: true,
            }],
        }
    }

    #[tokio::test]
    async fn materialize_plan_returns_reused_when_milestone_exists() {
        use crate::provider::github::client::mock::MockGitHubClient;
        use crate::provider::github::types::GhMilestone;

        let client = MockGitHubClient::new();
        client.set_existing_milestones(vec![GhMilestone {
            number: 42,
            title: "M0: Foundation".into(),
            description: String::new(),
            state: "open".into(),
            open_issues: 0,
            closed_issues: 0,
        }]);

        let plan = plan_with_one_issue();
        let result = materialize_plan_with_client(&plan, &client).await.unwrap();

        match result {
            MilestoneCreationResult::Reused {
                milestone_number,
                new_issue_numbers,
                skipped_issue_numbers,
            } => {
                assert_eq!(milestone_number, 42);
                assert_eq!(new_issue_numbers.len(), 1);
                assert!(skipped_issue_numbers.is_empty());
            }
            other => panic!("expected Reused, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn materialize_plan_skips_duplicate_issues_within_reused_milestone() {
        use crate::provider::github::client::mock::MockGitHubClient;
        use crate::provider::github::types::{GhIssue, GhMilestone};

        let client = MockGitHubClient::new();
        client.set_existing_milestones(vec![GhMilestone {
            number: 42,
            title: "M0: Foundation".into(),
            description: String::new(),
            state: "open".into(),
            open_issues: 0,
            closed_issues: 0,
        }]);
        client.set_existing_issues(vec![GhIssue {
            number: 99,
            title: "feat: login page".into(),
            body: String::new(),
            labels: vec![],
            state: "open".into(),
            html_url: String::new(),
            milestone: Some(42),
            assignees: vec![],
        }]);

        let plan = plan_with_one_issue();
        let result = materialize_plan_with_client(&plan, &client).await.unwrap();

        let MilestoneCreationResult::Reused {
            new_issue_numbers,
            skipped_issue_numbers,
            ..
        } = result
        else {
            panic!("expected Reused variant");
        };
        assert!(new_issue_numbers.is_empty());
        assert_eq!(skipped_issue_numbers, vec![99]);
    }

    // ---- #455 plan_validation_error ----

    #[test]
    fn plan_validation_rejects_empty_milestone_title() {
        let mut s = MilestoneWizardScreen::new();
        s.apply_planning_result(Ok(AiGeneratedPlan {
            milestone_title: "   ".into(),
            milestone_description: "d".into(),
            issues: vec![],
        }));
        assert!(s.plan_validation_error().is_some());
    }

    #[test]
    fn plan_validation_rejects_bad_issue_title() {
        let mut s = MilestoneWizardScreen::new();
        s.apply_planning_result(Ok(AiGeneratedPlan {
            milestone_title: "M0: Foundation".into(),
            milestone_description: "d".into(),
            issues: vec![AiProposedIssue {
                title: "-bad".into(),
                overview: "x".into(),
                blocked_by: vec![],
                accepted: true,
            }],
        }));
        assert!(s.plan_validation_error().is_some());
    }

    #[test]
    fn plan_validation_passes_on_clean_plan() {
        let mut s = MilestoneWizardScreen::new();
        s.apply_planning_result(Ok(AiGeneratedPlan {
            milestone_title: "M0: Foundation".into(),
            milestone_description: "d".into(),
            issues: vec![AiProposedIssue {
                title: "feat: login".into(),
                overview: "x".into(),
                blocked_by: vec![],
                accepted: true,
            }],
        }));
        assert!(s.plan_validation_error().is_none());
    }

    #[test]
    fn begin_materialization_stops_on_invalid_plan() {
        let mut s = MilestoneWizardScreen::new();
        s.apply_planning_result(Ok(AiGeneratedPlan {
            milestone_title: "   ".into(),
            milestone_description: "d".into(),
            issues: vec![],
        }));
        s.begin_materialization();
        assert_eq!(s.step(), MilestoneWizardStep::Failed);
        assert!(s.failure_reason().is_some());
    }
}
