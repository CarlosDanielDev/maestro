use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub const FOOTER_HINTS: &str = "[Enter] next   [Tab] cycle   [Shift+Enter] newline   [Esc] back";

#[derive(Debug, Clone, Copy)]
pub struct WizardFrameHeader<'a> {
    pub step_index: usize,
    pub step_total: usize,
    pub step_label: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct WizardFrameFooter<'a> {
    pub validation_error: Option<&'a str>,
    pub hints: Option<&'a str>,
}

pub struct WizardFrame;

impl WizardFrame {
    pub fn draw<F>(
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        header: WizardFrameHeader<'_>,
        footer: WizardFrameFooter<'_>,
        body_callback: F,
    ) where
        F: FnOnce(&mut Frame, Rect),
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(area);

        Self::draw_header(f, chunks[0], theme, header);
        body_callback(f, chunks[1]);
        Self::draw_footer(f, chunks[2], theme, footer);
    }

    fn draw_header(f: &mut Frame, area: Rect, theme: &Theme, header: WizardFrameHeader<'_>) {
        let title = format!(
            "Step {}/{}: {}",
            header.step_index, header.step_total, header.step_label
        );
        let block = theme.styled_block(&title, false);
        f.render_widget(Paragraph::new("").block(block), area);
    }

    fn draw_footer(f: &mut Frame, area: Rect, theme: &Theme, footer: WizardFrameFooter<'_>) {
        let hint = footer.hints.unwrap_or(FOOTER_HINTS);
        let lines = if let Some(err) = footer.validation_error {
            vec![
                Line::from(Span::styled(
                    err,
                    Style::default()
                        .fg(theme.accent_error)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    hint,
                    Style::default()
                        .fg(theme.text_secondary)
                        .add_modifier(Modifier::DIM),
                )),
            ]
        } else {
            vec![Line::from(Span::styled(
                hint,
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::DIM),
            ))]
        };
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
    }
}

#[cfg(test)]
mod tests {
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
}
