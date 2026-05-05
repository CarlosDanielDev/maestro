use super::*;
use crate::tui::screens::Screen;
use crate::tui::screens::issue_wizard::{IssueWizardScreen, IssueWizardStep};
use crate::tui::theme::Theme;
use insta::assert_snapshot;

#[test]
fn issue_wizard_type_select_uses_wizard_frame() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueWizardScreen::new();
    screen.set_step_for_tests(IssueWizardStep::TypeSelect);

    terminal.draw(|f| {
        screen.draw(f, f.area(), &theme);
    })?;

    assert_snapshot!(terminal.backend());
    Ok(())
}
