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

/// #1282: the five-step stepper, the Type row between Title and Overview,
/// and the Draft checklist column (Wide is 150 columns and up).
#[test]
fn issue_wizard_basics_wide() -> Result<(), Box<dyn std::error::Error>> {
    let mut screen = IssueWizardScreen::new();
    screen.paste_text_into_focused("Add a gauge widget");

    let terminal = draw_issue_wizard(&mut screen, 168, 30)?;
    let text = buffer_text(terminal.backend().buffer());
    let stepper = text.lines().next().unwrap_or_default();
    assert!(
        stepper.contains("● Basics ─ ○ Details ─ ○ Blocked by ─ ○ AI review ─ ○ Preview"),
        "five steps:\n{text}"
    );
    assert!(!text.contains("Context"), "{text}");
    assert!(text.contains("Draft"), "checklist column:\n{text}");
    assert!(text.contains("No [TODO] placeholders"), "{text}");
    assert!(text.contains("● Feature"), "type row:\n{text}");

    assert_snapshot!(terminal.backend());
    Ok(())
}

/// Below 100 columns the stepper collapses and the checklist is one strip.
#[test]
fn issue_wizard_basics_compact_80x24() -> Result<(), Box<dyn std::error::Error>> {
    let mut screen = IssueWizardScreen::new();
    let terminal = draw_issue_wizard(&mut screen, 80, 24)?;
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("Step 1/5 · Basics"), "{text}");
    assert!(text.contains("Draft 1/5 ready"), "ready strip:\n{text}");
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

/// #1454: AI review before any call — what it does, its cost, and the key.
/// Reached through `handle_key` and drawn by `ui::draw` at 80x24.
#[tokio::test]
async fn issue_wizard_ai_review_idle() -> Result<(), Box<dyn std::error::Error>> {
    use crate::tui::app::TuiMode;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut screen = IssueWizardScreen::new();
    screen.payload_mut().title = "Add gauge widget".into();
    screen.payload_mut().overview = "Render progress".into();
    screen.payload_mut().expected_behavior = "Fills proportionally".into();
    screen.payload_mut().acceptance_criteria = "- Renders 0..100".into();
    screen.set_step_for_tests(IssueWizardStep::Dependencies);
    screen.begin_dependency_fetch();
    screen.apply_dep_issues(Vec::new());
    let mut app = crate::tui::make_test_app("issue-wizard-1454");
    app.tui_mode = TuiMode::IssueWizard;
    app.screen_state.issue_wizard_screen = Some(screen);
    crate::tui::input_handler::handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .await;
    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    terminal.draw(|f| crate::tui::ui::draw(f, &mut app))?;
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("r runs the review"), "{text}");
    assert_snapshot!(terminal.backend());
    Ok(())
}
