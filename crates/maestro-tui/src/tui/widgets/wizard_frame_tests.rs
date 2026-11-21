use super::*;
use insta::assert_snapshot;
use ratatui::{
    Terminal,
    backend::TestBackend,
    widgets::{Block, Borders, Paragraph},
};

fn render_frame(
    validation_error: Option<&'static str>,
) -> Result<Terminal<TestBackend>, Box<dyn std::error::Error>> {
    let mut terminal = Terminal::new(TestBackend::new(80, 12))?;
    let theme = Theme::dark();
    terminal.draw(|f| {
        WizardFrame::draw(
            f,
            f.area(),
            &theme,
            WizardFrameHeader {
                step_index: 2,
                step_total: 10,
                step_label: "Type Select",
            },
            WizardFrameFooter {
                validation_error,
                hints: None,
            },
            |f, area| {
                f.render_widget(
                    Paragraph::new("Body slot").block(Block::default().borders(Borders::ALL)),
                    area,
                );
            },
        );
    })?;
    Ok(terminal)
}

#[test]
fn renders_styled_step_header_and_hints() -> Result<(), Box<dyn std::error::Error>> {
    let terminal = render_frame(None)?;
    assert_snapshot!(terminal.backend());
    Ok(())
}

#[test]
fn validation_error_preserves_hints() -> Result<(), Box<dyn std::error::Error>> {
    let terminal = render_frame(Some("Title is required"))?;
    assert_snapshot!(terminal.backend());

    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    assert!(text.contains("Title is required"));
    assert!(text.contains("[Enter] next"));
    Ok(())
}

use crate::tui::ds::checklist::CheckState;

const STEPS: &[&str] = &["Goals", "References", "Plan", "Create"];
const ITEMS: &[CheckItem<'static>] = &[
    CheckItem {
        label: "Title",
        state: CheckState::Ok,
        note: None,
    },
    CheckItem {
        label: "Goals",
        state: CheckState::Missing,
        note: Some("required"),
    },
    CheckItem {
        label: "Plan",
        state: CheckState::Pending,
        note: None,
    },
];

fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn render_stepped<F>(
    width: u16,
    checklist: bool,
    body: F,
) -> Result<Terminal<TestBackend>, Box<dyn std::error::Error>>
where
    F: FnOnce(&mut Frame, Rect, &Theme),
{
    render_stepped_with(width, STEPS, checklist, body)
}

fn render_stepped_with<F>(
    width: u16,
    steps: &[&str],
    checklist: bool,
    body: F,
) -> Result<Terminal<TestBackend>, Box<dyn std::error::Error>>
where
    F: FnOnce(&mut Frame, Rect, &Theme),
{
    let mut terminal = Terminal::new(TestBackend::new(width, 24))?;
    let theme = Theme::dark();
    terminal.draw(|f| {
        WizardFrame::draw_stepped(
            f,
            f.area(),
            &theme,
            WizardStepHeader {
                steps,
                current: 0,
                errors: &[0],
                state: Some(Span::raw("● draft saved 12s ago")),
            },
            checklist.then_some(WizardChecklist {
                title: "Draft",
                items: ITEMS,
            }),
            |f, area| body(f, area, &theme),
        );
    })?;
    Ok(terminal)
}

#[test]
fn wizard_frame_checklist_wide_only() {
    let cases = [
        (90, Breakpoint::Compact),
        (120, Breakpoint::Regular),
        (160, Breakpoint::Wide),
    ];
    for (width, bp) in cases {
        let area = Rect::new(0, 0, width, 20);
        let with = layout(area, bp, true);
        let wide = bp == Breakpoint::Wide;
        assert_eq!(with.checklist.is_some(), wide, "{bp:?}");
        assert_eq!(with.strip.is_some(), !wide, "{bp:?}");
        assert_eq!(with.stepper.height, 1, "{bp:?}");
        if let Some(col) = with.checklist {
            assert_eq!(col.width, CHECKLIST_WIDTH);
            assert_eq!(with.body.width + col.width, width);
        }
        let without = layout(area, bp, false);
        assert_eq!(without.checklist, None, "{bp:?}");
        assert_eq!(without.strip, None, "{bp:?}");
        assert_eq!(without.body.height, 19, "{bp:?}");
    }

    let wide = screen_text(&render_stepped(160, true, |_, _, _| {}).unwrap());
    assert!(wide.contains("[ Draft ]"), "{wide}");
    assert!(!wide.contains("Draft 1/3 ready"), "{wide}");
    let regular = screen_text(&render_stepped(120, true, |_, _, _| {}).unwrap());
    assert!(!regular.contains("[ Draft ]"), "{regular}");
    assert!(regular.contains("Draft 1/3 ready"), "{regular}");
}

#[test]
fn stepped_frame_draws_stepper_and_state_slot() -> Result<(), Box<dyn std::error::Error>> {
    let text = screen_text(&render_stepped(120, false, |_, _, _| {})?);
    let first = text.lines().next().unwrap_or_default();
    assert!(first.contains("✗ Goals ─ ○ References"), "{first:?}");
    assert!(
        first.trim_end().ends_with("● draft saved 12s ago"),
        "{first:?}"
    );
    assert!(!text.contains("[Enter] next"), "no footer hints: {text}");
    Ok(())
}

#[test]
fn wizard_field_error_border() -> Result<(), Box<dyn std::error::Error>> {
    let terminal = render_stepped(80, false, |f, area, theme| {
        let rows = Layout::vertical([Constraint::Length(3), Constraint::Length(3)]).split(area);
        let title = field_block("Title *", true, Some("Title is required"), theme);
        f.render_widget(Paragraph::new("").block(title), rows[0]);
        let body = field_block("Body", false, None, theme);
        f.render_widget(Paragraph::new("Some text").block(body), rows[1]);
    })?;
    assert_snapshot!("wizard_field_error_border", terminal.backend());

    let text = screen_text(&terminal);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[3].contains("✗ Title is required"), "{:?}", lines[3]);
    assert!(!lines[6].contains('✗'), "{:?}", lines[6]);
    let buffer = terminal.backend().buffer();
    let theme = Theme::dark();
    assert_eq!(buffer[(0, 1)].fg, theme.accent_error, "error border colour");
    assert_eq!(
        buffer[(0, 4)].fg,
        theme.border_inactive,
        "clean border colour"
    );
    Ok(())
}

#[test]
fn state_slot_survives_a_long_stepper() -> Result<(), Box<dyn std::error::Error>> {
    // 89 columns: fits a 110-column row alone, not beside the 23-column state.
    let steps = [
        "Configure project",
        "Analyze sources",
        "Review the plan",
        "Write all files",
        "Done",
    ];
    let text = screen_text(&render_stepped_with(110, &steps, false, |_, _, _| {})?);
    let first = text.lines().next().unwrap_or_default();
    assert!(
        first.trim_end().ends_with("● draft saved 12s ago"),
        "{first:?}"
    );
    assert!(first.contains("Step 1/5 · Configure project"), "{first:?}");
    Ok(())
}

#[test]
fn wizard_frame_compact_80x24() -> Result<(), Box<dyn std::error::Error>> {
    let terminal = render_stepped(80, true, |f, area, theme| {
        let title = field_block("Title *", true, None, theme);
        f.render_widget(Paragraph::new("").block(title), Rect { height: 3, ..area });
    })?;
    assert_snapshot!("wizard_frame_compact_80x24", terminal.backend());
    Ok(())
}

#[test]
fn wizard_frame_wide_checklist() -> Result<(), Box<dyn std::error::Error>> {
    let terminal = render_stepped(160, true, |f, area, theme| {
        let title = field_block("Title *", true, None, theme);
        f.render_widget(Paragraph::new("").block(title), Rect { height: 3, ..area });
    })?;
    assert_snapshot!("wizard_frame_wide_checklist", terminal.backend());
    Ok(())
}
