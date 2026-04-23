pub mod ai_planning;
mod draw;
pub mod types;

pub use ai_planning::{build_planning_prompt, parse_planning_response};
pub use types::{AiGeneratedPlan, AiProposedIssue, MilestonePlanPayload, MilestoneWizardStep};

use super::{Screen, ScreenAction};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Frame, layout::Rect};

/// AI-guided wizard for milestone planning. #294 owns the scaffold
/// (steps, payload, AI launch). Review/Preview/Materializing/Complete
/// land in #297.
pub struct MilestoneWizardScreen {
    step: MilestoneWizardStep,
    payload: MilestonePlanPayload,
    /// In-progress doc-reference line being typed by the user.
    doc_buffer: String,
    /// Async planning state.
    planning_in_flight: bool,
    generated_plan: Option<AiGeneratedPlan>,
    failure_reason: Option<String>,
}

impl MilestoneWizardScreen {
    pub fn new() -> Self {
        Self {
            step: MilestoneWizardStep::default(),
            payload: MilestonePlanPayload::default(),
            doc_buffer: String::new(),
            planning_in_flight: false,
            generated_plan: None,
            failure_reason: None,
        }
    }

    pub fn step(&self) -> MilestoneWizardStep {
        self.step
    }

    pub fn payload(&self) -> &MilestonePlanPayload {
        &self.payload
    }

    pub fn payload_mut(&mut self) -> &mut MilestonePlanPayload {
        &mut self.payload
    }

    pub fn doc_buffer(&self) -> &str {
        &self.doc_buffer
    }

    pub fn is_planning_in_flight(&self) -> bool {
        self.planning_in_flight
    }

    pub fn has_generated_plan(&self) -> bool {
        self.generated_plan.is_some()
    }

    #[allow(dead_code)] // Reason: consumed by ReviewPlan rendering in #297
    pub fn generated_plan(&self) -> Option<&AiGeneratedPlan> {
        self.generated_plan.as_ref()
    }

    pub fn failure_reason(&self) -> Option<&str> {
        self.failure_reason.as_deref()
    }

    pub fn validation_error(&self) -> Option<&'static str> {
        match self.step {
            MilestoneWizardStep::GoalDefinition => {
                if self.payload.goals.trim().is_empty() {
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
    /// the buffer.
    pub fn commit_doc_buffer(&mut self) {
        let entry = self.doc_buffer.trim().to_string();
        if entry.is_empty() {
            return;
        }
        let valid = Self::validate_reference(&entry);
        self.payload.doc_references.push(entry);
        self.payload.doc_reference_valid.push(valid);
        self.doc_buffer.clear();
    }

    /// Begin an AI planning request. Caller (event loop) is responsible
    /// for actually spawning the work via `TuiCommand::LaunchAiPlanning`.
    pub fn start_planning(&mut self) {
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

    fn append_to_active(&mut self, c: char) {
        match self.step {
            MilestoneWizardStep::GoalDefinition => self.payload.goals.push(c),
            MilestoneWizardStep::NonGoals => self.payload.non_goals.push(c),
            MilestoneWizardStep::DocReferences => self.doc_buffer.push(c),
            _ => {}
        }
    }

    fn backspace_active(&mut self) {
        match self.step {
            MilestoneWizardStep::GoalDefinition => {
                self.payload.goals.pop();
            }
            MilestoneWizardStep::NonGoals => {
                self.payload.non_goals.pop();
            }
            MilestoneWizardStep::DocReferences => {
                self.doc_buffer.pop();
            }
            _ => {}
        }
    }

    fn newline_active(&mut self) {
        match self.step {
            MilestoneWizardStep::GoalDefinition => self.payload.goals.push('\n'),
            MilestoneWizardStep::NonGoals => self.payload.non_goals.push('\n'),
            _ => {}
        }
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
        let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            modifiers,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };

        match (self.step, code, *modifiers) {
            (_, KeyCode::Esc, _) => {
                if self.step.is_first() {
                    return ScreenAction::Pop;
                }
                self.retreat();
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
                self.newline_active();
            }
            (_, KeyCode::Enter, _) => {
                self.try_advance();
            }
            (
                MilestoneWizardStep::GoalDefinition
                | MilestoneWizardStep::NonGoals
                | MilestoneWizardStep::DocReferences,
                KeyCode::Backspace,
                _,
            ) => {
                self.backspace_active();
            }
            (
                MilestoneWizardStep::GoalDefinition
                | MilestoneWizardStep::NonGoals
                | MilestoneWizardStep::DocReferences,
                KeyCode::Char(c),
                m,
            ) if !m.contains(KeyModifiers::CONTROL) => {
                self.append_to_active(*c);
            }
            _ => {}
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
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
    fn goal_chars_append_to_payload_goals() {
        let mut s = MilestoneWizardScreen::new();
        type_chars(&mut s, "ship");
        assert_eq!(s.payload().goals, "ship");
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
        assert_eq!(s.payload().goals, "a\nb");
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
        assert!(MilestoneWizardScreen::validate_reference("https://example.com"));
        assert!(MilestoneWizardScreen::validate_reference("http://localhost"));
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
}
