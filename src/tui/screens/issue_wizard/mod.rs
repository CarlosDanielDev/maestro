mod draw;
pub mod types;

pub use types::{IssueCreationPayload, IssueType, IssueWizardStep};

use super::{Screen, ScreenAction};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Frame, layout::Rect};

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
}

pub struct IssueWizardScreen {
    step: IssueWizardStep,
    payload: IssueCreationPayload,
    /// Index into `step_fields()` for the focused field (0 if step has none).
    pub(super) focus: usize,
}

impl IssueWizardScreen {
    pub fn new() -> Self {
        Self {
            step: IssueWizardStep::default(),
            payload: IssueCreationPayload::new(),
            focus: 0,
        }
    }

    pub fn step(&self) -> IssueWizardStep {
        self.step
    }

    pub fn payload(&self) -> &IssueCreationPayload {
        &self.payload
    }

    #[allow(dead_code)] // Reason: payload mutated by AI/dependency steps in #295/#296
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

    pub fn focused_field(&self) -> Option<FieldId> {
        let fields = self.step_fields();
        if fields.is_empty() {
            None
        } else {
            fields.get(self.focus % fields.len()).copied()
        }
    }

    fn field_value_mut(&mut self, field: FieldId) -> &mut String {
        match field {
            FieldId::Title => &mut self.payload.title,
            FieldId::Overview => &mut self.payload.overview,
            FieldId::ExpectedBehavior => &mut self.payload.expected_behavior,
            FieldId::CurrentBehavior => &mut self.payload.current_behavior,
            FieldId::StepsToReproduce => &mut self.payload.steps_to_reproduce,
            FieldId::AcceptanceCriteria => &mut self.payload.acceptance_criteria,
            FieldId::FilesToModify => &mut self.payload.files_to_modify,
            FieldId::TestHints => &mut self.payload.test_hints,
        }
    }

    pub fn field_value(&self, field: FieldId) -> &str {
        match field {
            FieldId::Title => &self.payload.title,
            FieldId::Overview => &self.payload.overview,
            FieldId::ExpectedBehavior => &self.payload.expected_behavior,
            FieldId::CurrentBehavior => &self.payload.current_behavior,
            FieldId::StepsToReproduce => &self.payload.steps_to_reproduce,
            FieldId::AcceptanceCriteria => &self.payload.acceptance_criteria,
            FieldId::FilesToModify => &self.payload.files_to_modify,
            FieldId::TestHints => &self.payload.test_hints,
        }
    }

    /// Validate that the current step is allowed to advance. Returns the
    /// failure reason for the user (empty string when valid).
    pub fn validation_error(&self) -> Option<&'static str> {
        match self.step {
            IssueWizardStep::BasicInfo => {
                if self.payload.title.trim().is_empty() {
                    Some("Title is required")
                } else if self.payload.overview.trim().is_empty() {
                    Some("Overview is required")
                } else {
                    None
                }
            }
            IssueWizardStep::DorFields => {
                if self.payload.acceptance_criteria.trim().is_empty() {
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
        if self.validation_error().is_some() {
            return false;
        }
        if let Some(next) = self.step.next() {
            self.step = next;
            self.focus = 0;
            true
        } else {
            false
        }
    }

    pub fn retreat(&mut self) -> bool {
        if let Some(prev) = self.step.previous() {
            self.step = prev;
            self.focus = 0;
            true
        } else {
            false
        }
    }

    fn cycle_focus_forward(&mut self) {
        let len = self.step_fields().len();
        if len > 0 {
            self.focus = (self.focus + 1) % len;
        }
    }

    fn cycle_focus_backward(&mut self) {
        let len = self.step_fields().len();
        if len > 0 {
            self.focus = (self.focus + len - 1) % len;
        }
    }

    fn append_to_focused(&mut self, c: char) {
        if let Some(field) = self.focused_field() {
            self.field_value_mut(field).push(c);
        }
    }

    fn backspace_focused(&mut self) {
        if let Some(field) = self.focused_field() {
            self.field_value_mut(field).pop();
        }
    }

    fn newline_focused(&mut self) {
        if let Some(field) = self.focused_field() {
            if field.is_multiline() {
                self.field_value_mut(field).push('\n');
            }
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
        let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            modifiers,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };

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
            IssueWizardStep::BasicInfo | IssueWizardStep::DorFields => {
                match (code, *modifiers) {
                    (KeyCode::Esc, _) => {
                        self.retreat();
                    }
                    (KeyCode::Tab, _) => {
                        self.cycle_focus_forward();
                    }
                    (KeyCode::BackTab, _) => {
                        self.cycle_focus_backward();
                    }
                    (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
                        self.newline_focused();
                    }
                    (KeyCode::Enter, _) => {
                        self.try_advance();
                    }
                    (KeyCode::Backspace, _) => {
                        self.backspace_focused();
                    }
                    (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
                        self.append_to_focused(*c);
                    }
                    _ => {}
                }
            }
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
        assert_eq!(s.payload().title, "hello");
    }

    #[test]
    fn basic_info_tab_cycles_to_overview() {
        let mut s = at_basic_info();
        s.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        assert_eq!(s.focused_field(), Some(FieldId::Overview));
        type_string(&mut s, "x");
        assert_eq!(s.payload().overview, "x");
        assert_eq!(s.payload().title, "");
    }

    #[test]
    fn basic_info_backspace_removes_last_char() {
        let mut s = at_basic_info();
        type_string(&mut s, "hi");
        s.handle_input(&key_event(KeyCode::Backspace), InputMode::Insert);
        assert_eq!(s.payload().title, "h");
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
        assert_eq!(s.payload().overview, "a\nb");
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
        assert_eq!(s.payload().title, "ab");
    }

    // ---- DorFields ----

    fn at_dor_fields(issue_type: IssueType) -> IssueWizardScreen {
        let mut s = at_basic_info();
        s.payload_mut().issue_type = issue_type;
        s.payload_mut().title = "t".to_string();
        s.payload_mut().overview = "o".to_string();
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
}
