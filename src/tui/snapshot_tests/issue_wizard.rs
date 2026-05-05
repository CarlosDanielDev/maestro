use super::*;
use crate::tui::screens::Screen;
use crate::tui::screens::issue_wizard::{IssueWizardScreen, IssueWizardStep};
use crate::tui::theme::Theme;
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

fn draw_issue_wizard(
    screen: &mut IssueWizardScreen,
    width: u16,
    height: u16,
) -> Result<Terminal<TestBackend>, Box<dyn std::error::Error>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    let theme = Theme::dark();
    terminal.draw(|f| {
        screen.draw(f, f.area(), &theme);
    })?;
    Ok(terminal)
}

fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn issue_wizard_type_select_uses_wizard_frame() -> Result<(), Box<dyn std::error::Error>> {
    let mut screen = IssueWizardScreen::new();
    screen.set_step_for_tests(IssueWizardStep::TypeSelect);

    let terminal = draw_issue_wizard(&mut screen, TERM_WIDTH, TERM_HEIGHT)?;

    assert_snapshot!(terminal.backend());
    Ok(())
}

#[test]
fn issue_wizard_ai_review_loading_uses_braille_spinner() -> Result<(), Box<dyn std::error::Error>> {
    let mut screen = IssueWizardScreen::new();
    screen.set_step_for_tests(IssueWizardStep::AiReview);
    screen.set_spinner_context(3, true);
    screen.begin_ai_review();

    let terminal = draw_issue_wizard(&mut screen, TERM_WIDTH, TERM_HEIGHT)?;

    assert_snapshot!(terminal.backend());
    Ok(())
}

#[test]
fn issue_wizard_ai_improve_loading_uses_braille_spinner() -> Result<(), Box<dyn std::error::Error>>
{
    let mut screen = IssueWizardScreen::new();
    screen.set_step_for_tests(IssueWizardStep::AiReview);
    screen.set_spinner_context(3, true);
    screen.begin_ai_review();
    screen.apply_ai_review(Ok("critique text".into()));
    screen.begin_improve();

    let terminal = draw_issue_wizard(&mut screen, TERM_WIDTH, TERM_HEIGHT)?;

    assert_snapshot!(terminal.backend());
    Ok(())
}

#[test]
fn issue_wizard_ai_improve_error_wraps_on_narrow_buffer() -> Result<(), Box<dyn std::error::Error>>
{
    let mut screen = IssueWizardScreen::new();
    screen.set_step_for_tests(IssueWizardStep::AiReview);
    screen.begin_ai_review();
    screen.apply_ai_review(Ok("critique text".into()));
    screen.begin_improve();
    screen.apply_improve_result(Err(
        "json parser error expected string value near nested payload field".into(),
    ));

    let terminal = draw_issue_wizard(&mut screen, 30, 14)?;
    let text = buffer_text(terminal.backend().buffer());
    let error_lines = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.contains("json parser")
                || trimmed.contains("expected string")
                || trimmed.contains("near nested")
                || trimmed.contains("payload field")
        })
        .count();

    assert!(
        error_lines >= 2,
        "expected improve error to wrap across multiple visible lines, got {error_lines}\n{text}"
    );
    Ok(())
}
