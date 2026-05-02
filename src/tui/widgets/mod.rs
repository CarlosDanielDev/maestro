pub mod bypass_indicator;
mod ci_monitor;
mod dropdown;
mod list_editor;
mod number_stepper;
pub mod stats_bar;
mod text_input;
mod toggle;
pub mod unified_pr_toggle;

pub use ci_monitor::CiMonitorWidget;
pub use dropdown::Dropdown;
pub use list_editor::ListEditor;
pub use number_stepper::NumberStepper;
pub use text_input::TextInput;
pub use toggle::Toggle;

use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect};

use crate::tui::screens::settings::validation::ValidationFeedback;
use crate::tui::theme::Theme;

/// Action returned by a form widget after handling input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetAction {
    /// No state change occurred.
    None,
    /// The widget's value was changed.
    Changed,
    /// Widget requests the app enter Insert mode.
    RequestInsertMode,
    /// Widget requests the app return to Normal mode.
    RequestNormalMode,
}

/// Enum-dispatch container for all form widget types.
pub enum WidgetKind {
    Toggle(Toggle),
    TextInput(TextInput),
    NumberStepper(NumberStepper),
    Dropdown(Dropdown),
    ListEditor(ListEditor),
}

impl WidgetKind {
    pub fn handle_input(&mut self, key: KeyEvent) -> WidgetAction {
        match self {
            Self::Toggle(w) => w.handle_input(key),
            Self::TextInput(w) => w.handle_input(key),
            Self::NumberStepper(w) => w.handle_input(key),
            Self::Dropdown(w) => w.handle_input(key),
            Self::ListEditor(w) => w.handle_input(key),
        }
    }

    pub fn draw(
        &self,
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        focused: bool,
        validation: Option<&ValidationFeedback>,
    ) {
        match self {
            Self::Toggle(w) => w.draw(f, area, theme, focused),
            Self::TextInput(w) => w.draw(f, area, theme, focused, validation),
            Self::NumberStepper(w) => w.draw(f, area, theme, focused, validation),
            Self::Dropdown(w) => w.draw(f, area, theme, focused),
            Self::ListEditor(w) => w.draw(f, area, theme, focused),
        }
    }

    #[allow(dead_code)] // Reason: widget label accessor — standard API surface
    pub fn label(&self) -> &str {
        match self {
            Self::Toggle(w) => &w.label,
            Self::TextInput(w) => &w.label,
            Self::NumberStepper(w) => &w.label,
            Self::Dropdown(w) => &w.label,
            Self::ListEditor(w) => &w.label,
        }
    }

    pub fn needs_insert_mode(&self) -> bool {
        match self {
            Self::TextInput(w) => w.editing,
            Self::ListEditor(w) => w.editing,
            _ => false,
        }
    }

    /// Short `(key, label)` hint describing how to edit the focused widget.
    pub fn edit_hint(&self) -> (&'static str, &'static str) {
        match self {
            Self::Toggle(_) => ("Space", "Toggle"),
            Self::Dropdown(_) => ("←/→", "Change"),
            Self::NumberStepper(_) => ("←/→", "Adjust"),
            Self::TextInput(_) => ("Enter", "Edit"),
            Self::ListEditor(_) => ("Enter", "Edit list"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_hint_toggle() {
        let w = WidgetKind::Toggle(Toggle::new("x", false));
        assert_eq!(w.edit_hint(), ("Space", "Toggle"));
    }

    #[test]
    fn edit_hint_dropdown() {
        let w = WidgetKind::Dropdown(Dropdown::new("x", vec!["a".into()], 0));
        assert_eq!(w.edit_hint(), ("←/→", "Change"));
    }

    #[test]
    fn edit_hint_number_stepper() {
        let w = WidgetKind::NumberStepper(NumberStepper::new("x", 1, 0, 10));
        assert_eq!(w.edit_hint(), ("←/→", "Adjust"));
    }

    #[test]
    fn edit_hint_text_input() {
        let w = WidgetKind::TextInput(TextInput::new("x", ""));
        assert_eq!(w.edit_hint(), ("Enter", "Edit"));
    }

    #[test]
    fn edit_hint_list_editor() {
        let w = WidgetKind::ListEditor(ListEditor::new("x", vec![]));
        assert_eq!(w.edit_hint(), ("Enter", "Edit list"));
    }
}
